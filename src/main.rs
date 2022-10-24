use std::collections::HashMap;

use anyhow::{anyhow, Context, Result};
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
struct ExtractedAndTaggedObject {
    object_name: String,
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
        .context("Failed to init Kubernetes client")?;

    let namespaces: Api<Namespace> = Api::all(client.clone());
    let namespace_list: Vec<String> = namespaces
        .list(&ListParams::default())
        .await
        .context("Failed to list namespaces")?
        .iter()
        .map(|ns| ns.clone().metadata.name.expect("Expected namespace name"))
        .collect();

    let mut join_handles = Vec::new();

    for namespace in namespace_list {
        let handle = tokio::spawn(get_pods(client.clone(), namespace.clone()));
        join_handles.push(handle);
    }

    let pod_level_results: Vec<ExtractedAndTaggedObject> = try_join_all(join_handles)
        .await
        .context("Failed to execute tokio task for retrieving pods in namespace")?
        .into_iter()
        .collect::<Result<Vec<ObjectList<Pod>>>>()
        .context("Failed to get pods in namespace")?
        .iter()
        .flatten()
        .map(|pod| {
            let pod_metadata = &pod.metadata;
            let pod_name = pod_metadata
                .name
                .clone()
                .unwrap_or("no_pod_name".to_string());
            let pod_spec = if let Some(spec) = pod.spec.clone() {
                Ok(spec)
            } else {
                Err(anyhow!("No pod spec for pod {}", &pod_name))
            }
            .context("Unable to retrieve pod spec")?;
            let namespace = if let Some(inside) = &pod_metadata.namespace {
                Ok(inside)
            } else {
                Err(anyhow!("No namespace for pod {}", &pod_name))
            }
            .context("Unable to retrieve pod namespace")?;
            let owners: String = match &pod_metadata.owner_references {
                Some(references) => references
                    .iter()
                    .map(|reference| reference.clone().name)
                    .collect::<Vec<String>>()
                    .join(","),
                None => format!("pod:{}", &pod_name),
            };
            extract_containers_and_info(
                owners,
                namespace.to_string(),
                "pod".to_string(),
                &pod_spec,
                1,
            )
        })
        .collect::<Result<Vec<Vec<ExtractedAndTaggedObject>>>>()
        .context("Failed to extract containers and tag object")?
        .into_iter()
        .flatten()
        .collect();

    // Aggregate results at container_name level
    // let initial_map: HashMap<String, ExtractedAndTaggedObject> = HashMap::new();
    // let agg_map = pod_level_results.iter().fold(initial_map, |acc, x| {
    //     let mut output = acc.clone();
    //     let key = format!(
    //         "{}-{}-{}-{}",
    //         x.object_name, x.namespace, x.type_of, x.container_name
    //     );
    //     if let Some(inside) = acc.get(&key) {
    //         let mut modified = inside.clone();
    //         modified.replica_num = inside.replica_num + x.replica_num;
    //         modified.total_cores = inside.total_cores + x.total_cores;
    //         output.insert(key, modified);
    //     } else {
    //         output.insert(key, x.clone());
    //     };
    //     output
    // });

    // let mut container_level_results: Vec<ExtractedAndTaggedObject> =
    //     agg_map.values().cloned().collect();

    // container_level_results.sort_by(|a, b| b.total_cores.partial_cmp(&a.total_cores).unwrap());
    let container_level_results = agg_and_sort(&pod_level_results, &extract_container_level_key);

    let filtered: Vec<ExtractedAndTaggedObject> = container_level_results
        .clone()
        .into_iter()
        .filter(|x| !x.image_check || !x.node_selector_check || !x.qos_check)
        .collect();

    // Sort by resources without filtering
    let table: String;
    if args.disable_filter {
        table = Table::new(&container_level_results).to_string();
    } else {
        table = Table::new(&filtered).to_string();
    }

    if args.print_table {
        println!("{}", table);
    }

    // object level
    let object_level_results = agg_and_sort(&container_level_results, &extract_object_level_key);
    // let object_table = Table::new(&object_level_results).to_string();

    if args.generate_csv {
        let mut wtr =
            Writer::from_path("results_by_container_name.csv").expect("expected valid csv writer");
        for i in &container_level_results {
            wtr.serialize(i).expect("Able to write row")
        }

        let mut obj_wtr =
            Writer::from_path("results_by_object.csv").expect("expected valid csv writer");
        for i in &object_level_results {
            obj_wtr.serialize(i).expect("Able to write row")
        }
    }

    Ok(())
}

async fn get_pods(client: Client, namespace: String) -> Result<ObjectList<Pod>> {
    let lp = ListParams::default();
    let pods: Api<Pod> = Api::namespaced(client, &namespace);
    let pod_list = pods
        .list(&lp)
        .await
        .context("Expected results for pod list")?;
    Ok(pod_list)
}

// Extract container_level key for aggregation
fn extract_container_level_key(x: &ExtractedAndTaggedObject) -> String {
    format!(
        "{}-{}-{}-{}",
        x.object_name, x.namespace, x.type_of, x.container_name
    )
}

// Extract object_level key for aggregation
fn extract_object_level_key(x: &ExtractedAndTaggedObject) -> String {
    format!("{}-{}-{}", x.object_name, x.namespace, x.type_of)
}

fn agg_and_sort(
    input_vec: &Vec<ExtractedAndTaggedObject>,
    function: &dyn Fn(&ExtractedAndTaggedObject) -> String,
) -> Vec<ExtractedAndTaggedObject> {
    let initial_map: HashMap<String, ExtractedAndTaggedObject> = HashMap::new();
    let agg_map = input_vec.iter().fold(initial_map, |acc, x| {
        let mut output = acc.clone();
        let key = function(x);
        if let Some(inside) = acc.get(&key) {
            let mut modified = inside.clone();
            modified.replica_num = inside.replica_num + x.replica_num;
            modified.total_cores = inside.total_cores + x.total_cores;
            output.insert(key, modified);
        } else {
            output.insert(key, x.clone());
        };
        output
    });

    let mut results: Vec<ExtractedAndTaggedObject> = agg_map.values().cloned().collect();

    results.sort_by(|a, b| b.total_cores.partial_cmp(&a.total_cores).unwrap());
    results
}

fn convert_quantity_to_int(quantity: String) -> Result<f32> {
    match quantity {
        x if x.contains('m') => Ok(x
            .replace("m", "")
            .parse::<f32>()
            .context(format!("Failed to parse {} as int", &x))?),
        x if x.contains('g') => Ok(x
            .replace("g", "")
            .parse::<f32>()
            .context(format!("Failed to parse {} as int", &x))?
            * 1000.0),
        _ => Ok(quantity
            .parse::<f32>()
            .context(format!("Failed to parse {} as int", &quantity))?
            * 1000.0),
    }
}

fn extract_containers_and_info(
    name: String,
    namespace: String,
    type_of: String,
    pod_spec: &PodSpec,
    replicas: i32,
) -> Result<Vec<ExtractedAndTaggedObject>> {
    // qos class BestEffort?
    // let bad_qos_class = pod_spec.clone().
    // dockerhub image
    let node_selector_check = pod_spec.clone().node_selector.is_some();
    let node_selectors = match pod_spec.clone().node_selector {
        Some(node_selector) => format!("{:?}", node_selector),
        _ => "None".to_string(),
    };
    let containers = pod_spec.clone().containers;
    let results: Result<Vec<ExtractedAndTaggedObject>> = containers
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
                Some(resource) if resource.requests.is_some() => {
                    // println!("Resource request is {:?}", resource);
                    let requests = resource.requests.expect("Expected request");
                    let cpu_request = requests.get("cpu");
                    if let Some(inside) = cpu_request {
                        convert_quantity_to_int(inside.clone().0)
                    } else {
                        Ok(0.0)
                    }
                }
                _ => Ok(0.0),
            };
            if let Ok(inside) = cpu_request {
                Ok(ExtractedAndTaggedObject {
                    object_name: name.clone(),
                    namespace: namespace.clone(),
                    type_of: type_of.clone(),
                    container_name,
                    node_selectors: node_selectors.clone(),
                    node_selector_check,
                    qos_check,
                    image_check,
                    image_url: image,
                    total_cores: inside * replicas as f32 / 1000.0,
                    replica_num: replicas,
                })
            } else {
                Err(cpu_request.err().unwrap())
            }
        })
        .collect();
    results
}

fn is_ecr_image(image: &str) -> bool {
    image.contains("amazonaws.com/")
}

// is hosted in somebody's repo somewhere. we assume this is more reliable than dockerhub
fn is_hosted_image(image: &str) -> bool {
    image.contains("gcr.io") || image.contains("quay.io") || image.contains("ghcr.io")
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::core::v1::{Container, ResourceRequirements};
    // use k8s_openapi::apimachinery::pkg::api::resource::quantity;
    use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
    use std::collections::BTreeMap;

    #[test]
    fn test_is_ecr_image() {
        let input =
            "095116963143.dkr.ecr.ap-southeast-1.amazonaws.com/datadog-agent:7.32.4".to_string();
        assert!(is_ecr_image(&input));
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

        let tagged_result = extract_containers_and_info(
            "random name".to_string(),
            "namespace".to_string(),
            "deployment".to_string(),
            &pod_spec,
            10,
        )
        .unwrap();

        assert_ne!(tagged_result.len(), 0);
        let x = tagged_result.first().unwrap();
        assert!(x.image_check);
        assert!(x.node_selector_check);
        assert!(x.qos_check);
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

        let tagged_result = extract_containers_and_info(
            "random name".to_string(),
            "namespace".to_string(),
            "deployment".to_string(),
            &pod_spec,
            10,
        )
        .unwrap();

        assert_ne!(tagged_result.len(), 0);
        let x = tagged_result.first().unwrap();
        assert!(x.image_check);
        assert!(x.node_selector_check);
        assert!(x.qos_check);
    }
}
