use backoff::backoff::Backoff;
use error::*;
use futures::FutureExt;
use futures_util::{pin_mut, select};
use k8s::PodExt;
use k8s_openapi::api::core::v1::Pod;
use result::Result;
use term_colors::*;
use tokio::sync::oneshot::{channel, Receiver, Sender};
use tokio::task::JoinHandle;
use tonic::transport::Endpoint;
use tonic_health::proto::health_client::HealthClient;

/// The maximum amount of time (in seconds) that well spend polling for the target
/// pod's gRPC server to become active.
pub const MAXIMUM_POLLING_TIME: u64 = 30;

/// A ServerCheck acts as a facade into the running coroutine that is polling for the newly
/// created connector pod gRPC endpoint.
pub struct ServerCheck {
    sigint: Sender<()>,
    handle: JoinHandle<()>,
}

impl ServerCheck {
    pub fn new(pod: &Pod) -> Result<(ServerCheck, Receiver<Result<()>>)> {
        let uri = format!("http://{}", pod.address()?);
        let endpoint: Endpoint = uri
            .parse()
            .map_err(|err| GrpcEndpointParsdeError { uri, source: err })?;
        let (sigint, sigint_rx) = channel();
        let (result_tx, result) = channel();
        let handle = tokio::spawn(Self::check(endpoint, sigint_rx, result_tx));
        Ok((ServerCheck { sigint, handle }, result))
    }

    /// Consumes this object and sends a shutdown signal to the background daemon that is
    /// polling the pods gRPC interface.
    ///
    /// This is useful if, say, the pod has been deleted. Since the daemon cannot tell the difference
    /// between a killed pod and a pod whose gRPC server hasn't come up yet, it needs to be told
    /// to just die when such an event occurs.
    pub async fn kill(self) {
        match self.sigint.send(()) {
            Err(err) => warn!(
                "The server health check coroutine appears have to shut itself \
            down earlier than expected. {:?}",
                err
            ),
            Ok(()) => (),
        };
        match self.handle.await {
            Ok(()) => (),
            Err(err) => {
                error!(
                    "The server health check coroutine appears to have panicked. {:?}",
                    err
                )
            }
        };
    }

    /// Consumes this object and waits for its backing coroutine to shutdown.
    ///
    /// An error will be logged if the coroutine panicked during its operation.
    pub async fn join(self) {
        match self.handle.await {
            Ok(()) => (),
            Err(err) => {
                error!(
                    "The server health check coroutine appears to have panicked. {:?}",
                    err
                )
            }
        };
    }

    /// Continuously polls the target gRPC endpoint following a strategy of exponential backoff.
    ///
    /// In order to be considered active, a gRPC endpoint must only RESPOND to a request. It does
    /// not need to respond with a SUCCESS. That is to say, this procedure is making a call into
    /// the standard [gRPC health check](https://github.com/grpc/grpc/blob/master/doc/health-checking.md)
    /// protocol. It does not yet REQUIRE that the target gRPC server actually implement the protocol
    /// (it is fine if the server responds with "method not found"), however it does require
    /// that connection can be established and that a response can be received at all.
    ///
    /// The MAXIMUM time that the gRPC endpoint has to become active is thirty seconds, at which
    /// point the pod will be considered ill-behaved.
    async fn check(endpoint: Endpoint, sigint: Receiver<()>, output: Sender<Result<()>>) {
        let mut latest_error = None;
        let mut b = backoff::ExponentialBackoff::default();
        b.max_elapsed_time = Some(std::time::Duration::from_secs(MAXIMUM_POLLING_TIME));
        let sigint = sigint.fuse();
        pin_mut!(sigint);
        loop {
            match b.next_backoff() {
                None => {
                    output
                        .send(Err(TooManyFailures {
                            uri: format!("{}", endpoint.uri()),
                            // This unwrap works ONLY because the only
                            // `continue` in this loop is immediately
                            // after assigning it a value. If a new
                            // continue is ever added or the extant
                            // continue moved, then this unwrap
                            // becomes unsafe.
                            source: latest_error.unwrap(),
                        }
                        .into()))
                        .unwrap();
                    return;
                }
                Some(duration) => {
                    let wait = tokio::time::sleep(duration).fuse();
                    pin_mut!(wait);
                    // Wait for either the next period in our exponential backoff
                    // or for us to receive a termination signal from the event watcher.
                    select! {
                        _ = wait => (),
                        _ = sigint => {
                            trace!("Server health check thread for {} received signal to shutdown \
                            while awaiting backoff timer", cyan(format!("{}", endpoint.uri())));
                            return;
                        }
                    };
                    // Attempt to establish a connection.
                    //
                    // In order to protect ourselves from a slow loris attack
                    // (https://en.wikipedia.org/wiki/Slowloris_(computer_security))
                    // we will compute the maximum allowable time (thirty seconds) minus how long
                    // we have waited thus far and assert that the connection MUST be established
                    // and responded to us before our "patience" runs out.
                    let connection = HealthClient::connect(endpoint.clone()).fuse();
                    let patience = tokio::time::Duration::from_secs(MAXIMUM_POLLING_TIME)
                        .checked_sub(b.get_elapsed_time())
                        .unwrap_or(tokio::time::Duration::from_secs(0));
                    let patience = tokio::time::sleep(patience).fuse();
                    pin_mut!(connection, patience);
                    // Either we have
                    // 1. Received a connection result.
                    // 2. Our patience ran out
                    // 3. Or we received a termination signal from the event watcher.
                    let conn = select! {
                        conn = connection => conn,
                        _ = patience => {
                            output.send(Err(NotReady {}.into())).unwrap();
                            return;
                        }
                        _ = sigint => {
                            trace!("Server health check thread for {} received signal to \
                            shutdown while awaiting server connection",
                                cyan(format!("{}", endpoint.uri())));
                            return;
                        }
                    };
                    // Alright! We got a result from the connection. But result could still
                    // something like "connection refused", meaning that the server is not up yet.
                    //
                    // So if we got an "Ok" then successfully established a connection!
                    // But if we got an "Err" then we should record what the error was and try
                    // again after the next backoff period.
                    match conn {
                        Ok(_) => {
                            output.send(Ok(())).unwrap();
                            return;
                        }
                        Err(err) => {
                            debug!(
                                "Could not connect to {}, {:?}",
                                cyan(format!("{}", endpoint.uri())),
                                err
                            );
                            latest_error = Some(err);
                            continue;
                        }
                    };
                }
            }
        }
    }
}

#[derive(Error, AcmError, Kind, Debug, HttpCode)]
#[error("")]
#[code(Status::ServiceUnavailable)]
pub struct NotReady {}

#[derive(Error, AcmError, Kind, Debug, HttpCode)]
#[error(
    "There were too many failures when attempting to connect to the requested pod \
({uri}) for its server health check"
)]
#[code(Status::ServiceUnavailable)]
pub struct TooManyFailures {
    uri: String,
    #[source]
    source: tonic::transport::Error,
}

#[derive(Error, AcmError, Kind, Debug, HttpCode)]
#[error(
    "There were too many failures when attempting to connect to the requested pod \
({uri}) for its server health check"
)]
#[code(Status::ServiceUnavailable)]
pub struct GrpcEndpointParsdeError {
    uri: String,
    #[source]
    source: k8s_openapi::http::uri::InvalidUri,
}
