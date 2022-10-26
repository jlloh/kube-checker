use anyhow::{anyhow, Context, Result};
use clap::Parser;
use csv::Writer;
use env_logger::Env;
use k8s_openapi::api::core::v1::{Namespace, Pod, PodSpec};
use kube::{
    api::{Api, ListParams, ObjectList},
    Client,
};
use log::info;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use tabled::{Table, Tabled};
use tokio::sync::mpsc::{channel, Sender};
mod utils;
use rayon::prelude::*;

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
    containers: String,
    node_selectors: String,
    node_selector_check: bool,
    qos_check: bool,
    image_check: bool,
    image_url: String,
    total_cores: f32,
    total_items: i32,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    let args = Args::parse();

    let client = Client::try_default()
        .await
        .context("Failed to init Kubernetes client")?;

    let namespaces: Api<Namespace> = Api::all(client.clone());
    info!("Retrieving namespaces");
    let namespace_list: Vec<String> = namespaces
        .list(&ListParams::default())
        .await
        .context("Failed to list namespaces")?
        .iter()
        .map(|ns| ns.clone().metadata.name.expect("Expected namespace name"))
        .collect();

    let mut join_handles = Vec::new();
    let namespace_len = &namespace_list.len();

    let (sender, mut receiver) = channel(*namespace_len);
    for namespace in namespace_list {
        let handle = tokio::spawn(get_pods(client.clone(), namespace.clone(), sender.clone()));
        join_handles.push(handle);
    }
    drop(sender);

    let (sender_2, mut receiver_2) = channel(*namespace_len);
    loop {
        // tokio::select! {
        let pod_list_result = receiver.recv().await;
        if let Some(pod_list) = pod_list_result {
            let pods = pod_list?;
            let sender = sender_2.clone();
            rayon::spawn(move || {
                info!("Spawning task to aggregate and sort results");
                let containers_result: Result<Vec<Vec<ExtractedAndTaggedObject>>> =
                    pods.into_iter().map(process_pod).collect();
                let aggregated = match containers_result {
                    Ok(containers) => Ok(agg_and_sort(
                        &containers.into_iter().flatten().collect(),
                        &extract_container_level_key,
                    )),
                    Err(e) => Err(e),
                };
                sender
                    .try_send(aggregated)
                    // .await
                    .expect("Failed send aggregated results from task");
            });
        } else {
            break;
        }
    }
    drop(sender_2);

    let mut container_level_results = Vec::new();
    loop {
        let results = receiver_2.recv().await;
        if let Some(inside) = results {
            let mut x = inside?;
            container_level_results.append(&mut x);
        } else {
            break;
        }
    }
    info!("Sorting container level results");
    container_level_results
        .par_sort_unstable_by(|a, b| b.total_cores.partial_cmp(&a.total_cores).unwrap());

    let filtered: Vec<ExtractedAndTaggedObject> = container_level_results
        .clone()
        .into_iter()
        .filter(|x| !x.image_check || !x.node_selector_check || !x.qos_check)
        .collect();

    // Sort by resources without filtering
    let table = if args.disable_filter {
        Table::new(&container_level_results).to_string()
    } else {
        Table::new(&filtered).to_string()
    };

    if args.print_table {
        println!("{}", table);
    }

    // object level
    info!("Aggregating results at object_level...");
    let mut object_level_results =
        agg_and_sort(&container_level_results, &extract_object_level_key);
    info!("Sorting object level results");
    object_level_results
        .par_sort_unstable_by(|a, b| b.total_cores.partial_cmp(&a.total_cores).unwrap());
    //     .unwrap();
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

fn process_pod(pod: Pod) -> Result<Vec<ExtractedAndTaggedObject>> {
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
            .join("|"),
        None => format!("pod:{}", &pod_name),
    };
    extract_containers_and_info(
        owners,
        namespace.to_string(),
        "pod".to_string(),
        &pod_spec,
        1,
    )
}

async fn get_pods(
    client: Client,
    namespace: String,
    sender: Sender<Result<ObjectList<Pod>>>,
) -> Result<()> {
    //Result<ObjectList<Pod>> {
    info!("Retrieving pods for namespace {}", &namespace);
    let lp = ListParams::default();
    let pods: Api<Pod> = Api::namespaced(client, &namespace);
    let pod_list = pods
        .list(&lp)
        .await
        .context("Expected results for pod list")?;
    info!("Finished retrieving pods for namespace {}", &namespace);
    sender
        .send(Ok(pod_list))
        .await
        .context("Failed to send pod results to channel")?;
    Ok(())
    // Ok(pod_list)
}

// Extract container_level key for aggregation
fn extract_container_level_key(x: &ExtractedAndTaggedObject) -> String {
    format!(
        "{}-{}-{}-{}",
        x.object_name, x.namespace, x.type_of, x.containers
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
            // It's a little inefficient to convert between hashset and string for every loop. But to be fixed later
            let mut current_items: HashSet<&str> = HashSet::from_iter(inside.containers.split('|'));
            let mut modified = inside.clone();
            modified.total_items = inside.total_items + x.total_items;
            modified.total_cores = inside.total_cores + x.total_cores;
            current_items.insert(&x.containers);
            modified.containers = Vec::from_iter(current_items).join("|");
            output.insert(key, modified);
        } else {
            output.insert(key, x.clone());
        };
        output
    });

    let mut results: Vec<ExtractedAndTaggedObject> = agg_map.values().cloned().collect();

    results.par_sort_unstable_by(|a, b| b.total_cores.partial_cmp(&a.total_cores).unwrap());
    results
}

fn convert_quantity_to_int(quantity: String) -> Result<f32> {
    match quantity {
        x if x.contains('m') => Ok(x
            .replace('m', "")
            .parse::<f32>()
            .context(format!("Failed to parse {} as int", &x))?),
        x if x.contains('g') => Ok(x
            .replace('g', "")
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
                    containers: container_name,
                    node_selectors: node_selectors.clone(),
                    node_selector_check,
                    qos_check,
                    image_check,
                    image_url: image,
                    total_cores: inside * replicas as f32 / 1000.0,
                    total_items: replicas,
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
