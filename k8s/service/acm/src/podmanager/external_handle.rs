use error::*;
use futures_util::{pin_mut, select, FutureExt};
use k8s_openapi::api::core::v1::Pod;
use rocket::tokio::task::JoinHandle;
use std::sync::Arc;

pub struct PodManagerUpperHandle {
    barrier: Arc<tokio::sync::Barrier>,
    result: tokio::sync::mpsc::Receiver<result::Result<Pod>>,
    phantom: Option<result::Result<Pod>>,
}

impl PodManagerLowerHandle {
    pub async fn send(
        &self,
        value: result::Result<Pod>,
    ) -> std::result::Result<(), tokio::sync::mpsc::error::SendError<result::Result<Pod>>> {
        self.result.send(value).await
    }
}

impl PodManagerUpperHandle {
    pub fn new() -> (PodManagerUpperHandle, PodManagerLowerHandle, JoinHandle<()>) {
        let barrier = Arc::new(tokio::sync::Barrier::new(2));
        let (tx1, mut rx1) = tokio::sync::mpsc::channel(1);
        let (tx2, rx2) = tokio::sync::mpsc::channel(1);
        let shim_barrier = barrier.clone();
        let handle_shim = tokio::spawn(async move {
            let result = match rx1.recv().await {
                None => {
                    let err = InboundResultChannelDropped {}.into();
                    error!("{}", err);
                    Err(err)
                }
                Some(result) => result,
            };
            let patience = tokio::time::Duration::from_secs(60);
            let patience = tokio::time::sleep(patience).fuse();
            let barrier = shim_barrier.wait().fuse();
            pin_mut!(patience, barrier);
            select! {
                _ = patience => {
                    return;
                },
                _ = barrier => ()
            }
            match tx2.send(result).await {
                Ok(()) => trace!("Successfully communicated pod result to the calling client"),
                Err(err) => error!("{}, {:?}", OutboundResultChannelDropped {}, err),
            }
        });
        let upper = PodManagerUpperHandle {
            barrier,
            result: rx2,
            phantom: None,
        };
        let lower = PodManagerLowerHandle { result: tx1 };
        (upper, lower, handle_shim)
    }

    pub async fn wait(&mut self) -> result::Result<Pod> {
        match self.phantom.as_ref() {
            None => (),
            Some(Ok(pod)) => return Ok(pod.clone()),
            Some(Err(_)) => return Err(PhantomError {}.into()),
        }
        self.barrier.wait().await;
        let result = match self.result.recv().await {
            Some(result) => result,
            None => Err(InboundResultChannelDropped {}.into()),
        };
        match result.as_ref() {
            Ok(pod) => {
                self.phantom = Some(Ok(pod.clone()));
            }
            Err(_) => {
                self.phantom = Some(Err(PhantomError {}.into()));
            }
        };
        result
    }
}

#[derive(Error, AcmError, Kind, HttpCode, Debug)]
#[code(Status::InternalServerError)]
#[error(
    "In internal datastructure was deallocated before a result was ever placed into. \
This is a severe state machine violation from within the ACM (Alation Connector Manager). \
Please try this operation again, but please also report this as a bug to Alation."
)]
pub struct InboundResultChannelDropped {}

#[derive(Error, AcmError, Kind, HttpCode, Debug)]
#[code(Status::InternalServerError)]
#[error(
    "In internal datastructure was deallocated before a result was ever placed into. \
This is a severe state machine violation from within the ACM (Alation Connector Manager). \
Please try this operation again, but please also report this as a bug to Alation."
)]
pub struct OutboundResultChannelDropped {}

#[derive(Error, AcmError, HttpCode, Kind, Debug)]
#[error("")]
#[code(Status::BadRequest)]
struct PhantomError {}

pub struct PodManagerLowerHandle {
    result: tokio::sync::mpsc::Sender<result::Result<Pod>>,
}
