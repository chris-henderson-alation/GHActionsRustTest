use super::server_check;

use crate::podmanager::external_handle::PodManagerLowerHandle;
use backoff::{backoff::Backoff, ExponentialBackoff};
use error::*;
use futures_util::{pin_mut, select, FutureExt, StreamExt, TryStreamExt};
use k8s::{client, PodExt};
use k8s_openapi::api::core::v1::Pod;
use kube::api::{DeleteParams, ListParams};
use kube::Api;
use result::Result;
use term_colors::*;
use tokio::task::JoinHandle;

/// An EventWatcher is a facade that may be used to communicate into
/// a running daemon that has registered itself as a listener with
/// the K8s API server for a given pod and is continually observing
/// the state of that pod.
pub struct EventWatcher {}

impl EventWatcher {
    /// In order to instantiate an EventWatcher it requires.
    ///
    ///     1. The ID of the pod. This MUST be the name of the pod in K8s
    ///         as it is used to retrieve an event stream over that pod.
    ///     2. The sender end of a channel of [GcStatus](GcStatus). The receiving end
    ///         of this channel MUST be given to garbage collector that pairs with this EventWatcher.
    ///     3. A PodManagerLowerHandle. This serves as the communication and synchronization
    ///         channel to external clients that may access results via the paired PodManagerUpperHandle.
    pub fn new_watcher<P: AsRef<str>>(
        pod_id: P,
        status: tokio::sync::mpsc::Sender<GcStatus>,
        lower: PodManagerLowerHandle,
    ) -> JoinHandle<()> {
        let event_watcher_daemon = EventWatcherDaemon {
            pod_id: pod_id.as_ref().to_string(),
            gc_status_signal: status,
            pod_manager_handle: lower,
        };
        tokio::spawn(event_watcher_daemon.watch())
    }
}

/// An EventWatcherDaemon is a simple holder of data for the ongoing coroutine that is the
/// actual daemon fired up via [watch](EventWatcherDaemon::watch).
struct EventWatcherDaemon {
    pod_id: String,
    gc_status_signal: tokio::sync::mpsc::Sender<GcStatus>,
    pod_manager_handle: PodManagerLowerHandle,
}

impl EventWatcherDaemon {
    /// watch is the actual event watcher damone coroutine.
    ///
    /// @TODO do a full writeup of everything we discussed, including swimlanes, and
    /// include and explanation of the comms channels setup between this, the GC, and
    /// the health checker.
    async fn watch(self) {
        let mut backoff = ExponentialBackoff::default();
        let client: Api<Pod> = client::new().await;
        let mut client = k8s::watcher::watcher(
            client,
            ListParams::default().fields(&format!("metadata.name={}", self.pod_id)),
        )
        .boxed();
        let mut pod = Pod::default();
        let start = tokio::time::Instant::now();
        ////////////////////////////////////////////////////////////////////////////
        // Phase 1
        ////////////////////////////////////////////////////////////////////////////
        loop {
            let next = client.try_next().await;
            let event = match next {
                Err(err) => match backoff.next_backoff() {
                    Some(duration) => {
                        warn!("Failure from the K8s API, {:?}", err);
                        tokio::time::sleep(duration).await;
                        continue;
                    }
                    None => {
                        error!("Too many failures from the K8s API, {:?}", err);
                        self.terminate(KubernetesUnresponsive {
                            elapsed: format!("{:?}", backoff.get_elapsed_time()),
                        })
                        .await;
                        return;
                    }
                },
                Ok(event) => event,
            };
            backoff.reset();
            let event = match event {
                None => {
                    // The stream is done? Kubernetes will never produce events
                    // again for this pod. I'm not entirely certain why this would
                    // happen, but it certainly seems like a terminal condition.
                    error!(
                        "Kubernetes has permanently closed the event stream for pod {} while the \
                    Event Watcher was in phase 1",
                        cyan(&self.pod_id)
                    );
                    self.terminate(UnexpectedCloseOfEventStream {}).await;
                    return;
                }
                Some(event) => event,
            };
            let p = match event {
                k8s::watcher::Event::Added(_) => {
                    // This is pretty much the very first event that
                    // occurs when you submit the deploy request to K8s.
                    trace!(
                        "Pod {} was added to the Kubernetes deployment queue",
                        cyan(&self.pod_id)
                    );
                    continue;
                }
                k8s::watcher::Event::Deleted(_) => {
                    // Yeah, this can happen if a client makes a call to
                    // `delete` before the pod even starts.
                    debug!(
                        "Pod {} was deleted from Kubernetes before it was ever deployed",
                        cyan(&self.pod_id)
                    );
                    self.terminate(PodDeleted {}).await;
                    return;
                }
                k8s::watcher::Event::Restarted(_) => {
                    // A "started" event gets reported as a "restart" event
                    // as well. Kind of confusing, yeah, but *shrug*.
                    //
                    // Note that "started" is NOT the same as running!
                    // We need to wait for the pod to be fully running!
                    trace!("Pod {} entered started/restarted state", cyan(&self.pod_id));
                    continue;
                }
                k8s::watcher::Event::Applied(pod) => pod,
            };
            if p.running() {
                pod = p;
                match self
                    .gc_status_signal
                    .send(GcStatus::Running(Box::new(pod.clone())))
                    .await
                {
                    Ok(_) => trace!(
                        "Garbage collector received {} signal for {}",
                        green("Running"),
                        cyan(&self.pod_id)
                    ),
                    Err(err) => {
                        let result = GarbageCollectorUnresponsive {
                            pod: self.pod_id.clone(),
                        };
                        error!("{}, {:?}", result, err);
                        self.terminate(result).await;
                        return;
                    }
                };
                info!(
                    "Pod {} entered the {} phase in {}",
                    cyan(&self.pod_id),
                    green("Running"),
                    orange(format!("{:?}", start.elapsed()))
                );
                trace!(
                    "State of pod {} upon entering running phase was: {:?}",
                    cyan(&self.pod_id),
                    pod
                );
                break;
            } else if p.terminated() || p.crashed() {
                let message = pod
                    .terminated_message()
                    .unwrap_or_else(|| "<None Given>".to_string());
                let reason = pod
                    .terminated_reason()
                    .unwrap_or_else(|| "<None Given>".to_string());
                info!(
                    "Pod {} entered the {} phase in {}",
                    cyan(&self.pod_id),
                    red("Terminated"),
                    orange(format!("{:?}", start.elapsed()))
                );
                debug!(
                    "Pod {} termination message: {}, reason: {}",
                    cyan(&self.pod_id),
                    message,
                    reason
                );
                trace!(
                    "The state of pod {} upon termination phase was: {:?}",
                    cyan(&self.pod_id),
                    pod
                );
                self.terminate(PodCrashed {}).await;
                return;
            } else if p.was_err_image_pull() {
                self.terminate(
                    p.err_image_pull()
                        .expect_err("unsafe call to PodExt::err_image_pull"),
                )
                .await;
                return;
            } else {
                continue;
            }
        }
        ////////////////////////////////////////////////////////////////////////////
        // Phase 2
        ////////////////////////////////////////////////////////////////////////////
        let (check, outcome) = match server_check::ServerCheck::new(&pod) {
            Ok((check, outcome)) => (check, outcome),
            Err(err) => {
                self.terminate(err).await;
                return;
            }
        };
        let outcome = outcome.fuse();
        pin_mut!(outcome);
        loop {
            let next_event = client.try_next().fuse();
            pin_mut!(next_event);
            let event: Phase2Event = select! {
                event = next_event => Phase2Event::K8s(event),
                status = outcome => Phase2Event::HealthCheck(status),
            };
            match event {
                Phase2Event::K8s(event) => match event {
                    Err(err) => match backoff.next_backoff() {
                        Some(duration) => {
                            warn!("Failure from the K8s API, {:?}", err);
                            tokio::time::sleep(duration).await;
                            continue;
                        }
                        None => {
                            // The API server has been busted for 15 minutes straight.
                            error!("Too many failures from the K8s API, {:?}", err);
                            check.kill().await;
                            self.terminate(KubernetesUnresponsive {
                                elapsed: format!("{:?}", backoff.get_elapsed_time()),
                            })
                            .await;
                            return;
                        }
                    },
                    Ok(Some(k8s::watcher::Event::Deleted(_))) => {
                        // This can easily happen if a client calls the delete
                        // endpoint before calling on the wait endpoint.
                        check.kill().await;
                        self.terminate(PodDeleted {}).await;
                        return;
                    }
                    Ok(Some(k8s::watcher::Event::Restarted(_))) => {
                        // It got restarted? We're not going to tolerate a boot cycle here.
                        check.kill().await;
                        self.terminate(PodRebooted {}).await;
                        return;
                    }
                    Ok(None) => {
                        // The stream is done? Kubernetes will never produce events
                        // again for this pod. I'm not entirely certain why this would
                        // happen, but it certainly seems like a terminal condition.
                        check.kill().await;
                        error!(
                            "Kubernetes has permanent closed the event stream for pod {} \
                        while the Event Watcher was in phase 1",
                            cyan(&self.pod_id)
                        );
                        self.terminate(UnexpectedCloseOfEventStream {}).await;
                        return;
                    }
                    // Reset the backoff in-case we had some failures because obviously
                    // now we're back online with the API server.
                    Ok(Some(_)) => backoff.reset(),
                },
                Phase2Event::HealthCheck(server_status) => match server_status {
                    Err(recv_error) => {
                        // This means that the server status coroutine dropped its sender.
                        // The connector may-or-may not be running, but our current state
                        // cannot be trusted as this is a severe violation of the state
                        // machine.
                        error!(
                            "Server status coroutine dropped its sender! {:?}",
                            recv_error
                        );
                        check.join().await;
                        self.terminate(HealthCheckDroppedItsChannel {}).await;
                        return;
                    }
                    Ok(Err(err)) => {
                        // The server health check has reported that it considers the
                        // the pod to be ill-behaved, and as such should be terminated.
                        check.join().await;
                        self.terminate(err).await;
                        return;
                    }
                    Ok(Ok(())) => {
                        // The server health check has reported that it considers the
                        // the pod to be alive and responsive.
                        check.join().await;
                        // Inform the upstream waiting client that their pod is ready.
                        match self.send_result(Ok(pod.clone())).await {
                            Ok(()) => (),
                            Err(err) => {
                                error!(
                                    "The server health check returned a response of a \
                                successful start. However, the upstream channel that communicates \
                                those results back to clients appears to have been closed early. \
                                Since we cannot communicate back to the client, there is nothing \
                                for us to do but show down the pod. {:?}",
                                    err
                                );
                                self.kill_gc().await;
                                self.kill_pod().await;
                                return;
                            }
                        }
                        break;
                    }
                },
            }
        }
        ////////////////////////////////////////////////////////////////////////////
        // Phase 3
        ////////////////////////////////////////////////////////////////////////////
        info!(
            "Pod {} completed its health check and came fully online in {}",
            cyan(&self.pod_id),
            orange(format!("{:?}", start.elapsed()))
        );
        loop {
            let next = client.try_next().await;
            let event = match next {
                Err(err) => match backoff.next_backoff() {
                    Some(duration) => {
                        warn!("Failure from the K8s API, {:?}", err);
                        tokio::time::sleep(duration).await;
                        continue;
                    }
                    None => {
                        error!("Too many failures from the K8s API, {:?}", err);
                        self.terminate(KubernetesUnresponsive {
                            elapsed: format!("{:?}", backoff.get_elapsed_time()),
                        })
                        .await;
                        return;
                    }
                },
                Ok(event) => event,
            };
            backoff.reset();
            let event = match event {
                None => {
                    // The stream is done? Kubernetes will never produce events
                    // again for this pod. I'm not entirely certain why this would
                    // happen, but it certainly seems like a terminal condition.
                    error!(
                        "Kubernetes has permanent closed the event stream for pod {} \
                    while the Event Watcher was in phase 3",
                        cyan(&self.pod_id)
                    );
                    self.terminate(UnexpectedCloseOfEventStream {}).await;
                    return;
                }
                Some(event) => event,
            };
            match event {
                k8s::watcher::Event::Deleted(_) => {
                    // Cool, the client appears to be done with the pod
                    // and it has been deleted. There is nothing left
                    // for us to do but shutdown the garbage collector.
                    self.kill_gc().await;
                    return;
                }
                k8s::watcher::Event::Restarted(_) => {
                    // It got restarted? We're not going to tolerate a boot cycle here.
                    self.terminate(PodRebooted {}).await;
                    return;
                }
                // We are not particularly interested in other events that may
                // occur during the rest of its lifecycle.
                _ => (),
            };
        }
    }

    /// Sends the final result to any waiting upstream client, kills the garbage collector,
    /// and tears down the pod being monitored.
    async fn terminate<T: Into<Box<dyn AcmError>>>(&self, err: T) {
        let _ = self.send_result(Err(err.into())).await;
        self.kill_gc().await;
        self.kill_pod().await;
    }

    /// Sends a shutdown signal the garbage collector. It is NOT fatal call this procedure
    /// if the GC has already been shutdown for any other reason.
    async fn kill_gc(&self) {
        match self.gc_status_signal.send(GcStatus::Terminated).await {
            Ok(_) => (),
            Err(err) => trace!(
                "The event watcher for pod {} sent a shutdown \
            signal to its garbage collector, however the garbage collector appears to have shut \
            itself down earlier than expected. {:?}",
                cyan(&self.pod_id),
                err
            ),
        }
    }

    /// Submits a request to Kubernetes to destroy the pod being monitored.
    async fn kill_pod(&self) {
        let client: Api<Pod> = client::new().await;
        let _ = client.delete(&self.pod_id, &DeleteParams::default()).await;
    }

    /// Sends the provided to result back upstream to any client that may be waiting.
    async fn send_result<T: Into<Result<Pod>>>(&self, err: T) -> Result<()> {
        self.pod_manager_handle
            .send(err.into())
            .await
            .map_err(|_| SendChannelClose {}.into())
    }
}

#[derive(Clone, Debug)]
/// A GcStatus us a simple binary status that may be sent to
/// the garbage collector to communicate start/shutdown signals.
pub enum GcStatus {
    Running(Box<Pod>),
    Terminated,
}

#[derive(Error, AcmError, HttpCode, Kind, Debug)]
#[error("")]
#[code(error::Status::InternalServerError)]
struct SendChannelClose {}

#[derive(Error, AcmError, HttpCode, Kind, Debug)]
#[error(
    "The connector has crashed. Please review its logs for additional debugging information \
and report any finding to the connector's development team for further analysis."
)]
#[code(error::Status::ServiceUnavailable)]
struct PodCrashed {}

enum Phase2Event {
    K8s(std::result::Result<Option<k8s::watcher::Event<Pod>>, k8s::watcher::Error>),
    HealthCheck(std::result::Result<Result<()>, tokio::sync::oneshot::error::RecvError>),
}

#[derive(Error, AcmError, HttpCode, Kind, Debug)]
#[code(error::Status::ServiceUnavailable)]
#[error(
    "The pod for this job was terminated before it ever entered the running state \
(perhaps it crashed immediately). The (optional) reason given by Kubernetes was '{reason}' \
and the (optional) message given was '{message}'."
)]
struct PodTerminatedBeforeStart {
    message: String,
    reason: String,
}

#[derive(Error, AcmError, HttpCode, Kind, Debug)]
#[code(error::Status::InternalServerError)]
#[error(
    "The garbage collector for the requested pod ({pod}) exited earlier than expected. \
For the sake of safety, the pod for this job has been deleted. Please try this operation again, \
but please also report this as a bug to the Alation OCF development and infrastructure team."
)]
struct GarbageCollectorUnresponsive {
    pod: String,
}

#[derive(Error, AcmError, HttpCode, Kind, Debug)]
#[code(error::Status::InternalServerError)]
#[error(
    "The Kubernetes API server has failed to reconnect for this job's event stream for over \
{elapsed}. The cluster appears to be too unhealthy to reasonably continue at this time."
)]
struct KubernetesUnresponsive {
    elapsed: String,
}

#[derive(Error, AcmError, HttpCode, Kind, Debug)]
#[code(error::Status::InternalServerError)]
#[error(
    "The Kubernetes API server has prematurely, and permanently, close the event stream for this \
job's pod. The job may work if simply re-ran, however this may be indicative of an unhealthy cluster."
)]
struct UnexpectedCloseOfEventStream {}

#[derive(Error, AcmError, HttpCode, Kind, Debug)]
#[code(error::Status::ServiceUnavailable)]
#[error(
"The pod for this job was deleted. If this not expected, then perhaps Alation timed out and the \
pod was garbage collected. Or perhaps another component has deleted the pod for some reason."
)]
struct PodDeleted {}

#[derive(Error, AcmError, HttpCode, Kind, Debug)]
#[code(error::Status::ServiceUnavailable)]
#[error(
"The pod for this job appears to have been rebooted. This may occur if the pod crashed and was \
restarted automatically. However, OCF has no tolerance for \"crashy\' connectors, and as such it \
has been deleted. Please gather logs for this connector and report the issue to the connector's \
development team."
)]
struct PodRebooted {}

#[derive(Error, AcmError, HttpCode, Kind, Debug)]
#[code(error::Status::InternalServerError)]
#[error(
"This job's server health check daemon shutdown prematurely, and without ever sending a status \
signal. This job may succeed if re-attempted, however this bug should be reported to the Alation \
OCF development and infrastructure team."
)]
struct HealthCheckDroppedItsChannel {}
