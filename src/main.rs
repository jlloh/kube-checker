use clap::Parser;
use csv::Writer;
use futures::future::try_join_all;
use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, StatefulSet};
use k8s_openapi::api::core::v1::{Namespace, PodSpec};
use kube::{
    api::{Api, ListParams, ObjectList},
    Client,
};
use serde::{Deserialize, Serialize};
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

#[derive(Debug, Clone)]
struct InputObject {
    name: String,
    namespace: String,
    type_of: String,
    pod_spec: PodSpec,
    replicas: i32,
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
async fn main() -> () {
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

    let mut waiters_deployments = Vec::new();
    let mut waiters_statefulsets = Vec::new();
    let mut waiters_daemonsets = Vec::new();

    for namespace in namespace_list {
        let handle_deployments = tokio::spawn(get_deployments(client.clone(), namespace.clone()));
        let handle_statefulsets = tokio::spawn(get_statefulsets(client.clone(), namespace.clone()));
        let handle_daemonsets = tokio::spawn(get_daemonsets(client.clone(), namespace.clone()));
        waiters_deployments.push(handle_deployments);
        waiters_statefulsets.push(handle_statefulsets);
        waiters_daemonsets.push(handle_daemonsets);
    }

    let results_deployments: Vec<InputObject> = try_join_all(waiters_deployments)
        .await
        .expect("Expected successful execution of async get deployments")
        .iter()
        .flatten()
        .map(|x| x.clone())
        .map(|deployment| {
            let replicas = match deployment
                .clone()
                .status
                .expect("Expected statefulset status")
                .replicas
            {
                Some(replica_num) => replica_num,
                _ => 0,
            };
            let pod_spec = deployment
                .clone()
                .spec
                .expect("Expected deployment spec")
                .template
                .spec
                .expect("Expected spec");
            let name = deployment
                .clone()
                .metadata
                .name
                .expect("Expected valid name");
            let namespace = deployment
                .clone()
                .metadata
                .namespace
                .expect("Expected namespace value");
            InputObject {
                name,
                namespace,
                type_of: "deployment".to_string(),
                pod_spec,
                replicas,
            }
        })
        .collect();

    let results_statefulsets: Vec<InputObject> = try_join_all(waiters_statefulsets)
        .await
        .expect("Expected successful execution of async get statefulsets")
        .iter()
        .flatten()
        .map(|x| x.clone())
        .map(|statefulset| {
            let replicas = statefulset
                .clone()
                .status
                .expect("Expected statefulset status")
                .replicas;
            let pod_spec = statefulset
                .clone()
                .spec
                .expect("Expected statefulset spec")
                .template
                .spec
                .expect("Expected podspec");
            let name = statefulset
                .clone()
                .metadata
                .name
                .expect("Expected valid name");
            let namespace = statefulset
                .clone()
                .metadata
                .namespace
                .expect("Expected namespace value");
            InputObject {
                name,
                namespace,
                type_of: "statefulset".to_string(),
                pod_spec,
                replicas,
            }
        })
        .collect();

    let mut full_results = results_deployments.clone();
    full_results.append(&mut results_statefulsets.clone());

    let mut tagged: Vec<TaggedObject> = full_results
        .into_iter()
        .map(|input| {
            tag_object(
                input.name,
                input.namespace,
                input.type_of,
                &input.pod_spec,
                input.replicas,
            )
        })
        .flatten()
        .collect();
    tagged.sort_by(|a, b| b.total_cores.partial_cmp(&a.total_cores).unwrap());

    let filtered: Vec<TaggedObject> = tagged
        .clone()
        .into_iter()
        .filter(|x| !x.image_check || !x.node_selector_check || !x.qos_check)
        .collect();

    // Sort by resources without filtering
    let table: String;
    if args.disable_filter {
        table = Table::new(&tagged).to_string();
    } else {
        table = Table::new(&filtered).to_string();
    }

    if args.print_table {
        println!("{}", table);
    }

    if args.generate_csv {
        let mut wtr = Writer::from_path("foo.csv").expect("expected valid csv writer");
        for i in &tagged {
            wtr.serialize(i).expect("Able to write row")
        }
    }
}

async fn get_deployments(client: Client, namespace: String) -> ObjectList<Deployment> {
    let lp = ListParams::default();
    let deployments: Api<Deployment> = Api::namespaced(client, &namespace);
    let deployment_list = deployments
        .list(&lp)
        .await
        .expect("Expected results for deployment list");
    return deployment_list;
}

// Copy pasta for now
async fn get_statefulsets(client: Client, namespace: String) -> ObjectList<StatefulSet> {
    let lp = ListParams::default();
    let deployments: Api<StatefulSet> = Api::namespaced(client, &namespace);
    let deployment_list = deployments
        .list(&lp)
        .await
        .expect("Expected results for deployment list");
    return deployment_list;
}

async fn get_daemonsets(client: Client, namespace: String) -> ObjectList<DaemonSet> {
    let lp = ListParams::default();
    let daemonsets: Api<DaemonSet> = Api::namespaced(client, &namespace);
    let daemonset_list = daemonsets
        .list(&lp)
        .await
        .expect("Expected results for daemonset list");
    return daemonset_list;
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
