use std::collections::HashMap;

use anyhow::{Context, Result};
use clap::Parser;
use csv::Writer;
use futures::future::try_join_all;
use k8s_openapi::api::core::v1::{Namespace, Pod, PodSpec};
use kube::{
    api::{Api, ListParams, ObjectList},
    Client,
};
use serde::Serialize;
use tabled::{Table, Tabled};

mod utils;

/// Clap command line arguments
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Disable filters so that all pods are displayed
    #[clap(long, takes_value = false)]
    disable_filter: bool,
    /// Generate a csv file in the directory, foo.csv
    #[clap(long, takes_value = false)]
    generate_csv: bool,
    /// Print out the table to stdout
    #[clap(long, takes_value = false)]
    print_table: bool,
}

#[derive(Debug, Tabled, Clone, Serialize)]
struct TaggedObject {
    name: String,
    namespace: String,
    type_of: String,
    container_name: String,
    node_selectors: String,
    node_selector_check: bool,
    qos_check: bool,
    image_check: bool,
    image_url: String,
    total_cores: f32,
    replica_num: i32,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let client = Client::try_default()
        .await
        .expect("Expected client init succeed");

    let namespaces: Api<Namespace> = Api::all(client.clone());
    let namespace_list: Vec<String> = namespaces
        .list(&ListParams::default())
        .await
        .expect("Expected namespace list")
        .iter()
        .map(|ns| ns.clone().metadata.name.expect("Expected namespace name"))
        .collect();

    let mut join_handles = Vec::new();

    for namespace in namespace_list {
        let handle = tokio::spawn(get_pods(client.clone(), namespace.clone()));
        join_handles.push(handle);
    }

    let pod_level_results: Vec<TaggedObject> = try_join_all(join_handles)
        .await
        .context("Failed to execute tokio task for retrieving pods in namespace")?
        .iter()
        .flatten()
        .map(|pod| {
            let pod_metadata = &pod.metadata;
            // TODO: remove unwrap
            let pod_spec = pod.spec.clone().unwrap();
            let owners: String = match &pod_metadata.owner_references {
                Some(references) => references
                    .iter()
                    .map(|reference| reference.clone().name)
                    .collect::<Vec<String>>()
                    .join(","),
                None => "unknown".to_string(),
            };
            tag_object(
                owners,
                // TODO: remove unwrap
                pod_metadata.clone().namespace.unwrap(),
                "pod".to_string(),
                &pod_spec,
                1,
            )
        })
        .flatten()
        .collect();

    // Aggregate results
    let initial_map: HashMap<String, TaggedObject> = HashMap::new();
    let agg_map = pod_level_results.iter().fold(initial_map, |acc, x| {
        let mut output = acc.clone();
        let key = format!(
            "{}-{}-{}-{}",
            x.name, x.namespace, x.type_of, x.container_name
        );
        if let Some(inside) = acc.get(&key) {
            let mut modified = inside.clone();
            modified.replica_num = inside.replica_num + x.replica_num;
            output.insert(key, modified);
        } else {
            output.insert(key, x.clone());
        };
        output
    });

    let mut final_vec: Vec<TaggedObject> = agg_map.values().cloned().collect();

    final_vec.sort_by(|a, b| b.total_cores.partial_cmp(&a.total_cores).unwrap());

    let filtered: Vec<TaggedObject> = final_vec
        .clone()
        .into_iter()
        .filter(|x| !x.image_check || !x.node_selector_check || !x.qos_check)
        .collect();

    // Sort by resources without filtering
    let table: String;
    if args.disable_filter {
        table = Table::new(&final_vec).to_string();
    } else {
        table = Table::new(&filtered).to_string();
    }

    if args.print_table {
        println!("{}", table);
    }

    if args.generate_csv {
        let mut wtr = Writer::from_path("foo.csv").expect("expected valid csv writer");
        for i in &final_vec {
            wtr.serialize(i).expect("Able to write row")
        }
    }

    Ok(())
}

async fn get_pods(client: Client, namespace: String) -> ObjectList<Pod> {
    let lp = ListParams::default();
    let pods: Api<Pod> = Api::namespaced(client, &namespace);
    let pod_list = pods.list(&lp).await.expect("Expected results for pod list");
    return pod_list;
}

fn convert_quantity_to_int(quantity: String) -> f32 {
    match quantity {
        x if x.contains("m") => x.replace("m", "").parse::<f32>().unwrap(),
        x if x.contains("g") => x.replace("g", "").parse::<f32>().unwrap() * 1000.0,
        _ => quantity.parse::<f32>().unwrap() * 1000.0,
    }
}

fn tag_object(
    name: String,
    namespace: String,
    type_of: String,
    pod_spec: &PodSpec,
    replicas: i32,
) -> Vec<TaggedObject> {
    // qos class BestEffort?
    // let bad_qos_class = pod_spec.clone().
    // dockerhub image
    let node_selector_check = pod_spec.clone().node_selector.is_some();
    let node_selectors = match pod_spec.clone().node_selector {
        Some(node_selector) => format!("{:?}", node_selector),
        _ => "None".to_string(),
    };
    let containers = pod_spec.clone().containers;
    let results: Vec<TaggedObject> = containers
        .into_iter()
        .map(|container| {
            let container_name = container.name;
            let image = container.image.unwrap();
            let image_check = is_ecr_image(&image) || is_hosted_image(&image);
            let qos_check = match &container.resources {
                Some(resource) => resource.requests.is_some(),
                None => false,
            };
            let cpu_request = match container.resources {
                Some(resource) if resource.requests.is_some() => convert_quantity_to_int(
                    resource
                        .requests
                        .expect("Expected requests")
                        .get("cpu")
                        .unwrap()
                        .clone()
                        .0,
                ),
                _ => 0.0,
            };
            TaggedObject {
                name: name.clone(),
                namespace: namespace.clone(),
                type_of: type_of.clone(),
                container_name,
                node_selectors: node_selectors.clone(),
                node_selector_check,
                qos_check,
                image_check,
                image_url: image,
                total_cores: cpu_request * replicas as f32 / 1000.0,
                replica_num: replicas,
            }
        })
        .collect();
    return results;
}

fn is_ecr_image(image: &str) -> bool {
    return image.contains("amazonaws.com/");
}

// is hosted in somebody's repo somewhere. we assume this is more reliable than dockerhub
fn is_hosted_image(image: &str) -> bool {
    return image.contains("gcr.io") || image.contains("quay.io") || image.contains("ghcr.io");
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::core::v1::{Container, ResourceRequirements};
    // use k8s_openapi::apimachinery::pkg::api::resource::quantity;
    use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
    use std::collections::BTreeMap;
    use utils;

    #[test]
    fn test_is_ecr_image() {
        let input =
            "095116963143.dkr.ecr.ap-southeast-1.amazonaws.com/datadog-agent:7.32.4".to_string();
        assert_eq!(is_ecr_image(&input), true);
    }

    // TODO: Make this ugly struct a yaml file? read from a yaml file and marshal into struct
    #[test]
    fn pass_case() {
        let mut node_selector = BTreeMap::new();
        node_selector.insert("key".to_string(), "value".to_string());

        let mut resource_requests = BTreeMap::new();
        resource_requests.insert("cpu".to_string(), Quantity("400m".to_string()));
        let pod_spec = PodSpec {
            containers: vec![Container {
                image: Some(
                    "095116963143.dkr.ecr.ap-southeast-1.amazonaws.com/datadog-agent:7.32.4"
                        .to_string(),
                ),
                name: "Test Name".to_string(),
                resources: Some(ResourceRequirements {
                    limits: None,
                    requests: Some(resource_requests),
                }),
                ..utils::get_empty_container()
            }],
            node_selector: Some(node_selector),
            ..utils::get_empty_pod_spec()
        };

        let tagged_result = tag_object(
            "random name".to_string(),
            "namespace".to_string(),
            "deployment".to_string(),
            &pod_spec,
            10,
        );

        assert_ne!(tagged_result.len(), 0);
        let x = tagged_result.first().unwrap();
        assert_eq!(x.image_check, true);
        assert_eq!(x.node_selector_check, true);
        assert_eq!(x.qos_check, true);
    }

    #[test]
    fn test_false_cases() {
        let mut resource_requests = BTreeMap::new();
        resource_requests.insert("cpu".to_string(), Quantity("400m".to_string()));
        let pod_spec = PodSpec {
            containers: vec![Container {
                image: Some("datadog-agent:7.32.4".to_string()),
                name: "Test Name".to_string(),
                resources: Some(ResourceRequirements {
                    limits: None,
                    requests: None,
                }),
                ..utils::get_empty_container()
            }],
            node_selector: None,
            ..utils::get_empty_pod_spec()
        };

        let tagged_result = tag_object(
            "random name".to_string(),
            "namespace".to_string(),
            "deployment".to_string(),
            &pod_spec,
            10,
        );

        assert_ne!(tagged_result.len(), 0);
        let x = tagged_result.first().unwrap();
        assert_eq!(x.image_check, false);
        assert_eq!(x.node_selector_check, false);
        assert_eq!(x.qos_check, false);
    }
}
