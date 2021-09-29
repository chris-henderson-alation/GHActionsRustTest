use k8s::client;
use k8s_openapi::api::core::v1::Pod;
use kube::api::ListParams;
use kube::Api;
use kube::ResourceExt;
use std::collections::HashSet;
use std::time::Duration;

#[allow(dead_code)]
pub async fn find_orphans() {
    tokio::time::sleep(Duration::from_secs(10)).await;
    let client: Api<Pod> = client::new().await;
    let pods = client.list(&ListParams::default()).await.unwrap();
    let client: Api<Pod> = client::new_for_system().await;
    let acms: HashSet<String> = client
        .list(
            &ListParams::default()
                .labels("app=acm")
                .fields("status.phase=Running"),
        )
        .await
        .unwrap()
        .into_iter()
        .map(|acm| acm.name())
        .collect();
    info!("{:?}", acms);
    for pod in pods {
        if !acms.contains(
            pod.metadata
                .labels
                .as_ref()
                .unwrap()
                .get("servicer")
                .unwrap(),
        ) {
            info!("I would have taken ownership of {}", pod.name())
        }
    }
}
