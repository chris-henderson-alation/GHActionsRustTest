pub mod client;
pub mod errors;
pub mod pod;
pub mod watcher;

pub use pod::PodExt;

use either::Either;
use kube::api::{DeleteParams, PostParams};
use kube::{Api, ResourceExt};
use result::Result;

use errors::ApiError;
use k8s_openapi::api::core::v1::Pod;
use kube::core::response::Status;
use kube::error::ErrorResponse;
use std::collections::BTreeMap;
use std::iter::FromIterator;

pub const OCF_NAMESPACE: &str = "ocf";
pub const OCF_SYSTEM_NAMESPACE: &str = "ocf-system";

/// Returns the pod object from the Kubernetes API server that is mapped
/// to the pod that actually executes this code. In this way, a caller with appropriate
/// ACLs to the namespace that it itself is operating in may do a bit of reflection
/// by retrieving its own pod.
///
/// This function uses the contents of /etc/hostname to retrieve the name of this pod.
/// Any error encountered while reading this file will panic the program since it is
/// simply not reasonable for it to not be available.
///
/// ```ignore
/// tokio_test::block_on(async {
///     let myself = servicer().await.unwrap();
///     assert_eq!(myself.metadata.name, tokio::fs::read_to_string("/etc/hostname").await.unwrap().trim());
/// })
/// ```
async fn servicer() -> Result<Pod> {
    let client: Api<Pod> = client::new_for_system().await;
    Ok(client
        .get(
            tokio::fs::read_to_string("/etc/hostname")
                .await
                .expect("could not read /etc/hostname! This is extremely fatal!")
                .trim(),
        )
        .await
        .map_err(|err| ApiError::from(err))?)
}

/// Deploys the given image reference to Kubernetes as a pod within the `ocf` namespace.
/// The provided `name` will be sanitized through the [rfc1123_subdomain](names::rfc1123_subdomain)
/// provided and then used as the `.metadata.name` of the newly created pod object.
///
/// The provided `ttl` is attached as additional metadata to the pod, but is otherwise not enacted
/// upon within this procedure.
///
/// The following `.metatdata.labels` are attached to each pod created through this function. More
/// may be added by upstream applications (such as the ACM's garbage collector adding an
/// `execution_date`.
///
/// * `servicer`: This is the `metadata.name` of the pod that created this new pod.
/// * `servicer_dns`: This is cluster DNS entry of the pod that created this new pod.
/// * `servicer_port`: This is listening port of the pod that created this new pod.
/// * `ttl`: The `ttl` passed into this function.
pub async fn deploy<R: AsRef<str>, N: AsRef<str>>(reference: R, name: N, ttl: u64) -> Result<Pod> {
    let mut pod = pod::new(reference, name)?;
    let myself = servicer().await?;
    pod.metadata.labels = Some(BTreeMap::from_iter([
        ("servicer".to_string(), myself.name()),
        ("servicer_dns".to_string(), myself.dns()?),
        ("servicer_port".to_string(), format!("{}", myself.port()?)),
        ("ttl".to_string(), format!("{}", ttl)),
    ]));
    let client: Api<Pod> = client::new().await;
    Ok(client
        .create(&PostParams::default(), &pod)
        .await
        .map_err(ApiError::from)?)
}

/// Delete a named resource
/// When you get a K via Left, your delete has started. When you get a Status via
/// Right, this should be a a 2XX style confirmation that the object being gone.
///
/// 4XX and 5XX status types are returned as an Err(Box<dyn AcmError>).
pub async fn delete<I: AsRef<str>>(id: I) -> Result<Either<Pod, Status>> {
    let client = client::new().await;
    Ok(client
        .delete(
            id.as_ref(),
            &DeleteParams {
                dry_run: false,
                grace_period_seconds: Some(60), // We return immediately, but the connector is given 60 seconds to shutdown cleanly.
                propagation_policy: None,
                preconditions: None,
            },
        )
        .await
        .or_else(|result| match result {
            kube::error::Error::Api(ErrorResponse { code: 404, .. }) => {
                Ok(Either::Right(kube::core::response::Status {
                    status: "".to_string(),
                    message: "".to_string(),
                    reason: "".to_string(),
                    details: None,
                    code: 0,
                }))
            }
            err => Err(err),
        })
        .map_err(ApiError::from)?)
}
