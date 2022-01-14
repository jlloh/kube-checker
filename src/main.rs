use futures::future::try_join_all;
use k8s_openapi::api::apps::v1::{Deployment, StatefulSet};
use k8s_openapi::api::core::v1::{Namespace, PodSpec};
use kube::{
    api::{Api, ListParams, ObjectList},
    Client,
};
use tabled::{Table, Tabled};

#[derive(Debug, Tabled)]
struct TaggedObject {
    name: String,
    type_of: String,
    container_name: String,
    node_selector_check: bool,
    qos_check: bool,
    image_check: bool,
}

#[tokio::main]
async fn main() -> () {
    // Infer the runtime environment and try to create a Kubernetes Client
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

    for namespace in namespace_list {
        let handle_deployments = tokio::spawn(get_deployments(client.clone(), namespace.clone()));
        let handle_statefulsets = tokio::spawn(get_statefulsets(client.clone(), namespace.clone()));
        waiters_deployments.push(handle_deployments);
        waiters_statefulsets.push(handle_statefulsets);
    }

    let results_deployments: Vec<Deployment> = try_join_all(waiters_deployments)
        .await
        .expect("Expected successful execution of async get deployments")
        .iter()
        .flatten()
        .map(|x| x.clone())
        .collect();
    // .filter(|x| filter_deployments(x))

    let tagged: Vec<TaggedObject> = results_deployments
        .into_iter()
        .map(|deployment| {
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
            tag_object(name.clone(), "deployment".to_string(), &pod_spec)
        })
        .flatten()
        .filter(|x| !x.image_check || !x.node_selector_check || !x.qos_check)
        .collect();

    let table = Table::new(tagged).to_string();
    println!("{}", table);

    // // TODO: Refactor this to avoid copy pasta
    // let results_statefulsets: Vec<StatefulSet> = try_join_all(waiters_statefulsets)
    //     .await
    //     .expect("Expected successful execution of async get deployments")
    //     .iter()
    //     .flatten()
    //     .map(|x| x.clone())
    //     .filter(|x| filter_statefulsets(x))
    //     .collect();

    // for statefulset in results_statefulsets {
    //     let metadata = statefulset.metadata;
    //     let name = metadata.clone().name.expect("Expected valid name");
    //     let namespace = metadata
    //         .clone()
    //         .namespace
    //         .expect("Expected valid namespace");

    //     println!("Statefulset {} in namespace {}", name, namespace);
    // }
}

async fn get_deployments(client: Client, namespace: String) -> ObjectList<Deployment> {
    let lp = ListParams::default();
    let deployments: Api<Deployment> = Api::namespaced(client, &namespace);
    let deployment_list = deployments
        .list(&lp)
        .await
        .expect("Expected results for deployment list");
    return deployment_list;
    // let results = deployment_list.iter()
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
    // let results = deployment_list.iter()
}

// fn filter_function(pod: &Pod) -> bool {
//     let no_node_selector = pod
//         .clone()
//         .spec
//         .expect("Expected spec")
//         .node_selector
//         .is_none();
//     return no_node_selector;
//     // return true;
// }

fn filter_deployments(deployment: &Deployment) -> bool {
    let pod_spec = deployment
        .clone()
        .spec
        .expect("Expected deployment spec")
        .template
        .spec
        .expect("Expected spec");
    let no_node_selector = pod_spec.clone().node_selector.is_none();
    // qos class BestEffort?
    // let bad_qos_class = pod_spec.clone().
    // dockerhub image
    return no_node_selector;
}

fn filter_statefulsets(deployment: &StatefulSet) -> bool {
    let pod_spec = deployment
        .clone()
        .spec
        .expect("Expected deployment spec")
        .template
        .spec
        .expect("Expected spec");
    let no_node_selector = pod_spec.clone().node_selector.is_none();
    // qos class BestEffort?
    // let bad_qos_class = pod_spec.clone().
    // dockerhub image
    return no_node_selector;
}

fn tag_object(name: String, type_of: String, pod_spec: &PodSpec) -> Vec<TaggedObject> {
    // qos class BestEffort?
    // let bad_qos_class = pod_spec.clone().
    // dockerhub image
    // let mut result = Vec::new();
    let node_selector_check = pod_spec.clone().node_selector.is_some();
    let containers = pod_spec.clone().containers;
    let results: Vec<TaggedObject> = containers
        .into_iter()
        .map(|container| {
            let container_name = container.name;
            let image = container.image.unwrap();
            let image_check = is_ecr_image(image);
            let qos_check = match container.resources {
                Some(resource) => resource.requests.is_some(),
                None => false,
            };
            TaggedObject {
                name: name.clone(),
                type_of: type_of.clone(),
                container_name,
                node_selector_check,
                qos_check,
                image_check,
            }
        })
        .collect();
    return results;
}

fn is_ecr_image(image: String) -> bool {
    return image.contains("amazonaws.com/");
}

// 095116963143.dkr.ecr.ap-southeast-1.amazonaws.com/datadog-agent:7.32.4
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
        assert_eq!(is_ecr_image(input), true);
    }

    // TODO: Make this ugly struct a yaml file? read from a yaml file and marshal into struct
    #[test]
    fn pass_case() {
        let mut node_selector = BTreeMap::new();
        node_selector.insert("key".to_string(), "value".to_string());

        let mut resource_requests = BTreeMap::new();
        resource_requests.insert("cpu".to_string(), Quantity("400m".to_string()));
        let pod_spec = PodSpec {
            active_deadline_seconds: None,
            affinity: None,
            automount_service_account_token: None,
            containers: vec![Container {
                args: None,
                command: None,
                env: None,
                env_from: None,
                image: Some(
                    "095116963143.dkr.ecr.ap-southeast-1.amazonaws.com/datadog-agent:7.32.4"
                        .to_string(),
                ),
                image_pull_policy: None,
                lifecycle: None,
                liveness_probe: None,
                name: "Test Name".to_string(),
                ports: None,
                readiness_probe: None,
                resources: Some(ResourceRequirements {
                    limits: None,
                    requests: Some(resource_requests),
                }),
                security_context: None,
                startup_probe: None,
                stdin: None,
                stdin_once: None,
                termination_message_path: None,
                termination_message_policy: None,
                tty: None,
                volume_devices: None,
                volume_mounts: None,
                working_dir: None,
            }],
            dns_config: None,
            dns_policy: None,
            enable_service_links: None,
            ephemeral_containers: None,
            host_aliases: None,
            host_ipc: None,
            host_network: None,
            host_pid: None,
            hostname: None,
            image_pull_secrets: None,
            init_containers: None,
            node_name: None,
            node_selector: Some(node_selector),
            overhead: None,
            preemption_policy: None,
            priority: None,
            priority_class_name: None,
            readiness_gates: None,
            restart_policy: None,
            runtime_class_name: None,
            scheduler_name: None,
            security_context: None,
            service_account: None,
            service_account_name: None,
            share_process_namespace: None,
            subdomain: None,
            termination_grace_period_seconds: None,
            tolerations: None,
            topology_spread_constraints: None,
            volumes: None,
        };

        let tagged_result = tag_object(
            "random name".to_string(),
            "deployment".to_string(),
            &pod_spec,
        );

        assert_ne!(tagged_result.len(), 0);
        let x = tagged_result.first().unwrap();
        assert_eq!(x.image_check, true);
        assert_eq!(x.node_selector_check, true);
        assert_eq!(x.qos_check, true);
    }
}
