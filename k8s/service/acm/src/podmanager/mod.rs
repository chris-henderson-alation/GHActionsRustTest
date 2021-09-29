use error::*;
use event_watcher::EventWatcher;
use external_handle::PodManagerUpperHandle;
use garbage_collector::GarbageCollector;
use garbage_collector::KeepAliveTicket;
use k8s_openapi::api::core::v1::Pod;
use result::Result;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use term_colors::*;
use tokio::join;
use tokio::sync::{Mutex, RwLock};

pub mod adoption;
pub mod event_watcher;
pub mod external_handle;
pub mod garbage_collector;
pub mod server_check;

lazy_static! {
    static ref POD_MANAGER_CACHE: RwLock<HashMap<String, Arc<Mutex<PodManager>>>> =
        RwLock::new(HashMap::new());
}

/// A PodManager holds two handles - one into the [garbage collection](GarbageCollector) daemon for a give pod
/// and one into the [event watcher](event_watcher::EventWatcher) daemon for a given pod. It
/// serves as a view for external clients (that is, clients over the ACM's HTTP interface) to \
/// interact with these two long running daemons.
///
/// Primarily, clients may interact with these components in two ways:
///
/// 1. They may [wait](PodManager::wait) for a pod to become active.
/// 2. They may [refresh](PodManager::refresh) the time-to-live for a given pod.
pub struct PodManager {
    gc_handle: GarbageCollector,
    event_watcher_handle: PodManagerUpperHandle,
}

impl PodManager {
    /// Retrieves the PodManager at the given ID should it exist. Should the PodManager
    /// not exist, then an Err([PodManagerNotFound](PodManagerNotFound)) is returned.
    ///
    pub async fn get<T: AsRef<str>>(id: T) -> Result<Arc<Mutex<PodManager>>> {
        POD_MANAGER_CACHE
            .read()
            .await
            .get(id.as_ref())
            .cloned()
            .ok_or_else(|| {
                PodManagerNotFound {
                    id: id.as_ref().to_string(),
                }
                .into()
            })
    }

    /// Instantiates a new PodManager. The PodManager that is created is NOT returned by this
    /// procedure. Rather, upon completion it will be immediately available via
    /// [PodManager::get](PodManager::get) using the same ID provided to this function.
    ///
    /// The `ttl` provided will be used as the initial value for the TTL in the garbage collector
    /// that will be spun up to back this new PodManager. If no specific TTL is desired, then
    /// one may use the [DEFAULT_TTL](garbage_collector::DEFAULT_TTL) defined in the garbage
    /// collector module.
    pub async fn new_podmanager<T: AsRef<str>>(id: T, ttl: u64) {
        // @TODO the object graph here could use some cleanup. The design pattern is
        // ALMOST consistent across the whole multiple components that comprise a Podmanager,
        // but not quite.
        let pod = id.as_ref().to_string();
        // pm_to_ew_send/recv is a pair of pseudo channels that are used for an external client
        // to reach through a PodManager and retrieve a "wait" result from the EventWatcher.
        // The returned "shim" is simply a coroutine that spinning that is maintaining this
        // communicate channel between the two objects. As such, a reference to it needs to be
        // held onto and eventually "joined" on to make sure that all PodManager coroutines
        // shutdown everytime.
        let (pm_to_ew_send, pm_to_ew_recv, shim) = PodManagerUpperHandle::new();
        // ew_to_gc_send/recv is the channel pair used for the EventWatcher to communicate to
        // the GarbageCollector. The EventWatcher gets the sending end of the channel and the
        // GarbageCollector gets the receiving end.
        let (ew_to_gc_send, ew_to_gc_recv) = tokio::sync::mpsc::channel(100);
        // Lets get our EventWatcher. This is a coroutine that needs to be eventually joined.
        let watcher_handle = EventWatcher::new_watcher(pod.clone(), ew_to_gc_send, pm_to_ew_recv);
        // Lets get our GarbageCollector. The "gc" is a facade into the actual garbage collector
        // while the "gc_handle" is a coroutine that needs to be eventually joined.
        let (gc, gc_handle) = GarbageCollector::new(ew_to_gc_recv, pod.clone(), ttl);
        let manager = PodManager {
            gc_handle: gc,
            event_watcher_handle: pm_to_ew_send,
        };
        let p = pod.clone();
        // This is the one coroutine that we spin off for which there is NO remaining
        // reference that we hold onto in memory. Once all couroutines that are backing
        // a given PodManager "join" (finish) then this coroutine proceeds forward and
        // deletes the PodManager from the in-memory cache. It then reports how
        // many PodManagers are still alive after this deletion. This log entry is
        // very useful for easily identifying whether or not all coroutines are successfully
        // shutting down and getting cleaned up. That is to say, once this ACM has gone entirely
        // idle, then this log entry MUST report that the number pod managers present eventually
        // winds down to zero. Otherwise, their is likely a rouge runtime somewhere.
        tokio::spawn(async move {
            let pod = p;
            let (_, _, _) = join!(watcher_handle, gc_handle, shim);
            let left_alive = {
                let mut managers = POD_MANAGER_CACHE.write().await;
                managers.remove(&pod);
                managers.len()
            };
            debug!(
                "PodManager for {} has been successfully cleaned up, {} are still alive",
                cyan(&pod),
                left_alive
            );
        });
        POD_MANAGER_CACHE
            .write()
            .await
            .insert(pod.clone(), Arc::new(Mutex::new(manager)));
    }

    /// Refreshes the TTL in the garbage collector for the pod managed by this PodManager.
    ///
    /// This is a straight passthroughs to [GarbageCollector::refresh](GarbageCollector::refresh).
    pub async fn refresh(&self) -> Result<KeepAliveTicket> {
        self.gc_handle.refresh().await
    }

    /// Waits for the pod to either become active or to be considered "ill-behaved".
    pub async fn wait(&mut self) -> Result<Pod> {
        self.event_watcher_handle.wait().await
    }
}

#[derive(Error, AcmError, HttpCode, Kind, Debug)]
#[code(Status::NotFound)]
#[error(
    "The pod manager for {id} could not be found. If an error occurred in the requested pod \
then the caller has one hour to consume the message before the record is dropped. This message \
may also only be consumed once. Alternatively, the calling client may have been configured for \
the incorrect ACM (Alation Connection Manager) that was not in possession of the requested \
pod manager."
)]
pub struct PodManagerNotFound {
    id: String,
}

/// A PodTicket is the simple combination of a pod strucutre as returned by
/// the Kubernetes API server and a [KeepAliveTicker](garbage_collector::KeepAliveTicket).
#[derive(Serialize, Kind)]
pub struct PodTicket {
    pub pod: Pod,
    pub ticket: garbage_collector::KeepAliveTicket,
}
