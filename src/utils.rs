// use kube::{
//     api::{Api, ListParams, ObjectList},
//     Client,
// };

// async fn get_k8s_object<Object: k8s_openapi::Metadata + std::clone::Clone>(
//     client: Client,
//     namespace: String,
// ) -> ObjectList<Object> {
//     let lp = ListParams::default();
//     let daemonsets: Api<Object> = Api::namespaced(client, &namespace);
//     let daemonset_list = daemonsets
//         .list(&lp)
//         .await
//         .expect("Expected results for daemonset list");
//     return daemonset_list;
// }

// Only used in test code for now, so this annotation is to surpress warnings from unused code
#[allow(dead_code)]
pub fn get_empty_pod_spec() -> k8s_openapi::api::core::v1::PodSpec {
    let pod_spec = k8s_openapi::api::core::v1::PodSpec {
        active_deadline_seconds: None,
        affinity: None,
        automount_service_account_token: None,
        containers: vec![],
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
        node_selector: None,
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
    return pod_spec;
}

#[allow(dead_code)]
pub fn get_empty_container() -> k8s_openapi::api::core::v1::Container {
    let container = k8s_openapi::api::core::v1::Container {
        args: None,
        command: None,
        env: None,
        env_from: None,
        image: None,
        image_pull_policy: None,
        lifecycle: None,
        liveness_probe: None,
        name: "Test Name".to_string(),
        ports: None,
        readiness_probe: None,
        resources: None,
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
    };
    return container;
}
