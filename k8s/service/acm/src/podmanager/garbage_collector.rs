use super::event_watcher::GcStatus;
use chrono::DateTime;
use chrono::Utc;
use error::*;
use futures::FutureExt;
use futures_util::{pin_mut, select};
use k8s::client;
use k8s_openapi::api::core::v1::Pod;
use kind::Kind;
use kube::api::{DeleteParams, Patch, PatchParams};
use kube::Api;
use result::Result;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::iter::FromIterator;
use std::ops::Add;
use term_colors::*;
use tokio::sync::mpsc;
use tokio::sync::oneshot::{channel, Sender};
use tokio::task::JoinHandle;

pub const DEFAULT_TTL: u64 = 60 * 30;

/// A `KeepAliveTicket` is issued to client programs who lease out pods. It encodes two pieces
/// of information intended for client consumption:
///
/// 1. A unique identifier used to refer to this ticket.
/// 2. A Unix timestamp which is the exact instant when this ticket becomes invalid.
#[derive(Serialize, Kind, Clone)]
pub struct KeepAliveTicket {
    /// `ticket` is the unique identifier for this `KeepAliveTicket`
    ///
    /// It is currently simply the name of the pod that it is tied
    /// to, however this should not be taken to be a guarantee of
    /// future behavior.
    ticket: String,
    /// `execution_date` is the Unix timestamp of the exact moment
    /// when the `KeepAliveTicket` becomes invalid and deletion of
    /// the backing pod will commence.
    ///
    /// There is no grace period.
    execution_date: i64,
    // Anything annotated with #[serde(skip)] will NOT
    // be serialized into the JSON returned to the client
    // when they receive one of these things.
    //
    // Fields used for pretty printing logs.
    #[serde(skip)]
    now: DateTime<Utc>,
    #[serde(skip)]
    then: DateTime<Utc>,
    // The actual object used to countdown our timer.
    #[serde(skip)]
    execution_instant: tokio::time::Instant,
}

impl KeepAliveTicket {
    /// A new `KeepAliveTicket` will be constructed with the name of the given pod
    /// as the ticket ID.
    ///
    /// Execution dates are computed against the give `ttl` at the moment of this
    /// procedure's execution.
    pub fn new<P: AsRef<str>>(pod: P, ttl: u64) -> KeepAliveTicket {
        let now = chrono::Utc::now();
        let then = now.add(chrono::Duration::seconds(ttl as i64));
        let execution_date = then.timestamp();
        let execution_instant =
            tokio::time::Instant::now().add(tokio::time::Duration::from_secs(ttl));
        let ticket = pod.as_ref().to_string();
        KeepAliveTicket {
            ticket,
            execution_date,
            now,
            then,
            execution_instant,
        }
    }

    /// Puts the running couroutine to sleep until the moment that `execution_instant` is reached.
    pub async fn sleep(self) {
        tokio::time::sleep_until(self.execution_instant).await;
    }

    /// Returns a (Patch<Pod>)[use kube::api::Patch] object that may be used to update
    /// a given pod with am accurate `.metadata.labels.execution_date`.
    ///
    /// This is especially useful for recording this information into Kubernetes itself
    /// so that disaster recovery may happen (for example, if this ACM dies then another
    /// instance of the ACM could reconstruct a PodManager using this information).
    fn pod_patch(&self) -> Patch<Pod> {
        let mut patch = Pod::default();
        patch.metadata.labels = Some(BTreeMap::from_iter([(
            "execution_date".to_string(),
            format!("{}", self.execution_date),
        )]));
        Patch::Merge(patch)
    }
}

/// The logging display implementation of a `KeepAliveTicket`. This dictates how to format
/// the object into a log entry when used with a `"{}"` formatting directive.
impl Display for KeepAliveTicket {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&format!(
            "Ticket created on {} (Unix {}) is good until {} (Unix {})",
            green(self.now.to_rfc2822()),
            self.now.timestamp(),
            red(self.then.to_rfc2822()),
            self.then.timestamp()
        ))
    }
}

/// A `GarbageCollector` is a facade over the long-running daemon that is tracking the garbage
/// collection status of a particular pod.
pub struct GarbageCollector {
    refresh_sender: mpsc::Sender<RefreshRequest>,
}

impl GarbageCollector {
    /// A new garbage collector takes in:
    ///
    /// 1. A receiver channel of [GcStatus](super::event_watcher::GcStatus)es. This channel servers
    ///     as the sole means of communication between the event watcher thread and the garbage
    ///     collector thread. The two signals that the event watcher may send to the GC are
    ///     [GcStatus::Running](super::event_watcher::GcStatus::Running) and [GcStatus::Terminated](super::event_watcher::GcStatus::Terminated).
    ///     These statuses are used the GC as go-ahead and shutdown signals.
    /// 2. The name of the pod being managed by this garbage collector.
    /// 3. The `ttl` interval for this garbage collector.
    ///
    /// A tuple of a `GarbageCollector` and a [JoinHandle<()>](tokio::task::JoinHandle) are returned.
    ///
    /// The `GarbageCollector` object is a facade into the running coroutin that is the actual
    /// garbage collector. It has a single method, [refresh](GarbageCollector::refresh), which may
    /// be used to reset the GC's execution date and retrieve a new [KeepAliveTicket](KeepAliveTicket).
    ///
    /// The return [JoinHandle<()>](tokio::task::JoinHandle) is the actual running coroutine that is
    /// the garbage collector. `await`ing on this handle will block indefinitely until the
    /// garbage collector exists.
    pub fn new(
        status: mpsc::Receiver<GcStatus>,
        pod: String,
        ttl: u64,
    ) -> (GarbageCollector, JoinHandle<()>) {
        let (refresh_sender, refresh_receiver) = mpsc::channel(1);
        let gc = GarbageCollector { refresh_sender };
        let gcd = GarbageCollectorDaemon {
            refresh_receiver,
            status,
        };
        (gc, tokio::spawn(gcd.gc(pod, ttl)))
    }

    /// Retrieves a refreshed [KeepAliveTicket](KeepAliveTicket).
    ///
    /// An [error](RefreshChannelClosed) will be returned in the extremely unlikely, although
    /// technically possible, event that the garbage collector has proceeded with a shutdown
    /// sequence at the exact same time that a client has requested a refresh.
    pub async fn refresh(&self) -> Result<KeepAliveTicket> {
        let (tx, rx) = channel();
        match self.refresh_sender.send(tx).await {
            Ok(()) => (),
            Err(_) => return Err(RefreshChannelClosed {}.into()),
        };
        match rx.await {
            Ok(ticket) => Ok(ticket),
            Err(_) => Err(RefreshChannelClosed {}.into()),
        }
    }
}

#[derive(Error, AcmError, HttpCode, Kind, Debug)]
#[code(Status::ServiceUnavailable)]
#[error("This pod appears to have already been shutdown or garbage collected.")]
pub struct RefreshChannelClosed {}

struct GarbageCollectorDaemon {
    refresh_receiver: mpsc::Receiver<RefreshRequest>,
    status: mpsc::Receiver<GcStatus>,
}

enum GcEvent {
    RefreshRequest(Option<RefreshRequest>),
    ExecutionDateReached,
    PodEvent(Option<GcStatus>),
}

impl GarbageCollectorDaemon {
    async fn gc(mut self, pod: String, ttl: u64) {
        /////////////////////////////////////////////////////////////////////////////////
        // Phase 1: Begin listening for an event received from the event watcher.
        //          At this point, the GC countdown has not begun because the pod
        //          has not even been provisioned yet.
        debug!(
            "GC waiting for go head to begin countdown for {}",
            cyan(&pod)
        );
        match self.status.recv().await {
            None => {
                // This is probably a bug should this occur. The event watcher shutdown
                // before ever giving a signal to the GC.
                warn!(
                    "GC received a signal that the event watcher for {} prematurely shutdown",
                    cyan(&pod)
                );
                return;
            }
            Some(GcStatus::Terminated) => {
                // The pod has shutdown before it ever even started. This'll happen for
                // instant crashes, bad images, etc.
                debug!(
                    "GC received {} signal for {}, shutting down",
                    stringify!(Status::Terminated),
                    cyan(&pod)
                );
                return;
            }
            Some(GcStatus::Running(_)) => {
                // Yay! The pod is running!
                debug!(
                    "GC received {} signal for {}, beginning routine",
                    stringify!(Status::Running),
                    cyan(&pod)
                );
            }
        };
        /////////////////////////////////////////////////////////////////////////////////
        // Phase 2: Instantiate a client and prepare a ticket for countdown.
        //          In this phase, we listen on three events:
        //
        //              1. The timeout has been reached, in which case the pod is destroyed.
        //              2. The event watcher signals that the pod has exited or been deleted,
        //                  in which case the GC simply exits.
        //              3. A refresh request has come in.
        let client: Api<Pod> = client::new().await;
        let mut keep_alive = KeepAliveTicket::new(&pod, ttl);
        info!(
            "Garbage collection for {} has been schedule. {}",
            cyan(&pod),
            keep_alive
        );
        client
            .patch(&pod, &PatchParams::default(), &keep_alive.pod_patch())
            .await
            .unwrap();
        loop {
            let timeout = keep_alive.clone().sleep().fuse();
            let refresh_request = self.refresh_receiver.recv().fuse();
            let status_change = self.status.recv().fuse();
            pin_mut!(timeout, refresh_request, status_change);
            // This right here is the magical select statement which chooses whichever event
            // occurs first.
            let event = select! {
                refresh = refresh_request => GcEvent::RefreshRequest(refresh),
                _ = timeout => GcEvent::ExecutionDateReached,
                status = status_change => GcEvent::PodEvent(status)
            };
            drop(timeout);
            match event {
                GcEvent::RefreshRequest(None) => {
                    // This would be a pretty bad bug should it ever occur. Unfortunately, by
                    // definition, it can't be communicated back to the caller because the
                    // comms channel was dropped early.
                    error!(
                        "A garbage collection refresh request was sent for {}, \
                    however its return channel was immediately dropped before a refreshed \
                    ticket could be generated. Please review the GarbageCollector::refresh \
                    method as this is a serious state machine violation.",
                        cyan(&pod)
                    );
                }
                GcEvent::RefreshRequest(Some(refresh)) => {
                    // A new refresh request came in.
                    keep_alive = KeepAliveTicket::new(&pod, ttl);
                    match refresh.send(keep_alive.clone()) {
                        Ok(()) => (),
                        Err(_) => error!("Failed to send a refresh ticket over a GC channel"),
                    };
                    client
                        .patch(&pod, &PatchParams::default(), &keep_alive.pod_patch())
                        .await
                        .unwrap();
                    info!(
                        "Garbage collection for {} has been refreshed. {}",
                        cyan(&pod),
                        keep_alive
                    );
                }
                GcEvent::PodEvent(None) => {
                    // The event listener went down without sending us a signal. This NOT
                    // what it is suppose to do, but just to be safe let's assume that it completely
                    // crashed and burned and now we need to be the ones to clean the pod up.
                    warn!("The event listener for pod {} has shutdown", cyan(&pod));
                    client.delete(&pod, &DeleteParams::default()).await.unwrap();
                    return;
                }
                GcEvent::PodEvent(Some(GcStatus::Running(_))) => {
                    // Neat? We shouldn't be receiving such superfluous signals, but it's
                    // not an error or nothing. It's just not useful.
                    debug!(
                        "Garbage collector received running signal for {} in mid-operation",
                        cyan(&pod)
                    );
                }
                GcEvent::PodEvent(Some(GcStatus::Terminated)) => {
                    // The pod has been deleted. Most commonly this is due to a client
                    // explicitly deleting the pod through the ACM's API.
                    debug!(
                        "Garbage collector received termination signal for {}",
                        cyan(&pod)
                    );
                    return;
                }
                GcEvent::ExecutionDateReached => {
                    // The timeout has been reached! Kill it!
                    warn!("Garbage collection timeout reached for {}", cyan(&pod));
                    client.delete(&pod, &DeleteParams::default()).await.unwrap();
                    return;
                }
            };
        }
    }
}

/// A RefreshRequest is channel on which a PodManager's daemon may return a new ticket
type RefreshRequest = Sender<KeepAliveTicket>;
