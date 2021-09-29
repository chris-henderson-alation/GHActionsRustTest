extern crate jemallocator;

// The use of jemalloc (http://jemalloc.net/) as the global allocator is actually QUITE
// important here. The glibc standard allocator cannot handle concurrency nearly as well,
// especially with regard to heap fragmentation.
//
// In particular, for the ACM, post peak usage has observed to be an issue when using glibc
// as the global allocator. "Peak" in this case is about 1500 connectors all requested at the same
// time. At it's height, memory usage spikes to ~700MB. After all 1500 connectors have been
// serviced, and the ACM is entirely idle again, glibc will "calm back down" to ~250MB idle.
// jemalloc, however, will gradual idle to ~45MB usage after such usage.
#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

pub mod podmanager;

use crate::podmanager::garbage_collector::KeepAliveTicket;
use crate::podmanager::{garbage_collector, PodManager, PodTicket};
use k8s_openapi::api::core::v1::Pod;
use kube::ResourceExt;
use response::Response;
use result::Result;
use term_colors::*;

#[macro_use]
extern crate rocket;

#[macro_use]
extern crate lazy_static;

/// A POST to the deploy endpoint will deploy the requested tag into Kubernetes using the
/// provided name as a prefix to the new pod (AFTER it has been sanitized via [rfc1123_subdomain](names::rfc1123_subdomain)).
///
/// An optional TTL may be provided which is the number of seconds that the pod is allowed to live
/// without a call to [refresh](self::refresh()). If no TTL is provided, then the
/// [default TTL](podmanager::garbage_collector::DEFAULT_TTL) is used.
///
/// The pod object returned by this endpoint is NOT ready for consumption. It has NOT been
/// provisioned by Kubernetes. It does NOT have an IP address. The result returned by this
/// endpoint is merely the PROMISE that the pod will eventually be provisioned. Client MUST
/// make a call to the [wait](self::wait()) endpoint before attempting any communication with
/// the request pod.
///
/// The garbage collection timeout does NOT begin immediately upon calling this endpoint. However,
/// it DOES begin immediately upon the pods actual creation in Kubernetes. However, sane clients
/// SHOULD be immediately making a call to [wait](self::wait()) which WILL refresh the garbage
/// collector's timeout for you on your behalf such that you are guaranteed to have full session
/// available to you once the pod has been confirmed to be fully functional.
///
/// ```text
/// curl -X POST http://acm.ocf-system/deploy?tag=abcd1234&SuperCoolConnector&ttl=150
/// ```
///
/// ```text
/// client = Client()
/// pod = client.deploy(connector)
/// pod.wait()
/// print(pod.address())
/// ```
#[post("/deploy?<tag>&<name>&<ttl>")]
pub async fn deploy(tag: String, name: String, ttl: Option<u64>) -> Result<Response<Pod>> {
    let registry = std::env::var("REGISTRY").unwrap_or("registry.kurl".to_string());
    let repository = std::env::var("REPOSITORY").unwrap_or("ocf".to_string());
    let reference = format!("{}/{}:{}", registry, repository, tag);
    let ttl = ttl.unwrap_or(garbage_collector::DEFAULT_TTL);
    let pod = k8s::deploy(reference, name, ttl).await?;
    podmanager::PodManager::new(pod.name(), ttl).await;
    Ok(pod.into())
}

/// A GET to the wait endpoint blocks INDEFINITELY until either the pod requested by [deploy](self::deploy())
/// is confirmed to be online and listening on a gRPC interface OR the pod is confirmed to be
/// "ill-behaved". What it means to be ill-behaved can be one or more of many possible scenarios
/// including (but not limited to):
///
/// * The pod crashed.
/// * The pod non-responsive on its gRPC interface
/// * The pod failed to deployed by Kubernetes
///
/// What it means for a successful response from this endpoint is that two things are guaranteed.
///
/// 1. The pod has entered its "running" phase and is active.
/// 2. The pod has responded over gRPC to a health check.
///
/// Of course, it is possible that sometime AFTER this call completes that the pod
/// crashes or becomes unresponsive, but at the very least the caller is guaranteed that
/// the pod has entered a reasonable state of execution.
///
/// Upon completion of this request the garbage collector timeout associated with this pod
/// will be automatically refreshed on the caller's behalf.
///
/// ```text
/// curl -X GET http://acm.ocf-system/wait?id=super-cool-connector-abcd12345
/// ```
///
/// ```text
/// client = Client()
/// pod = client.deploy(connector)
/// pod.wait()
/// print(pod.address())
/// ```
#[get("/wait?<id>")]
pub async fn wait(id: String) -> Result<Response<PodTicket>> {
    let lock = PodManager::get(&id).await?;
    let mut manager = lock.lock().await;
    let pod = manager.wait().await?;
    let ticket = manager.refresh().await?;
    Ok(PodTicket { pod, ticket }.into())
}

/// A POST to refresh resets the countdown timer for the associated ticket in the garbage collector.
/// The value used for the TTL is the (optional) value that was given to the call to
/// [deploy](self::deploy()) which created the pod that this ticket is for.
///
/// ```text
/// curl -X POST http://acm.ocf-system/refresh?ticket=super-cool-connector-abcd12345
/// ```
///
/// ```text
/// client = Client()
/// pod = client.deploy(connector)
/// pod.wait()
/// pod.refresh()
/// pod.refresh()
/// pod.refresh()
/// ```
#[post("/refresh?<ticket>")]
pub async fn refresh(ticket: String) -> Result<Response<KeepAliveTicket>> {
    Ok(PodManager::get(&ticket)
        .await?
        .lock()
        .await
        .refresh()
        .await?
        .into())
}

/// A DELETE to the delete endpoint destroys the pod in Kubernetes. This endpoint is idempotent,
/// meaning that clients may make as many calls to this endpoint as they like.
///
/// ```text
/// curl -X DELETE http://acm.ocf-system/delete?id=super-cool-connector-abcd12345
/// ```
///
/// ```text
/// client = Client()
/// pod = client.deploy(connector)
/// pod.wait()
/// pod.delete()
/// pod.delete()
/// pod.delete()
/// ```
#[delete("/delete?<id>")]
pub async fn delete(id: String) -> Result<Response<()>> {
    match k8s::delete(id.as_str()).await? {
        either::Left(pod) => info!("Deleting pod {}", cyan(pod.name())),
        either::Right(_) => info!("Pod {} was already deleted", cyan(id)),
    }
    Ok(().into())
}

#[tokio::main]
async fn main() {
    // Sets the logger to use terminal colors.
    std::env::set_var("RUST_LOG_STYLE", "always");
    env_logger::init();
    let mut c = rocket::Config::default();
    // If you leave it to the default then it will choose
    // 127.0.0.1 which will not be reachable whe running
    // in a container. So please leave this to 0.0.0.0.
    c.address = "0.0.0.0".parse().unwrap();
    rocket::custom(c)
        .mount("/", routes![deploy, wait, delete, refresh])
        .launch()
        .await
        .unwrap();
}
