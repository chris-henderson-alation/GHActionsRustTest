use crate::errors::ApiError;
use async_trait::async_trait;
use error::*;
use futures::stream::StreamExt;
use k8s_openapi::api::core::v1::Pod;
use kube::api::{LogParams, ObjectMeta};
use kube::core::Resource;
use kube::Api;
use kube::ResourceExt;
use std::path::Path;
use tokio::io::BufWriter;
use tokio_util::io::StreamReader;

/// Returns a new Kubernetes client configured for the [OCF Namespace](crate::OCF_NAMESPACE).
///
/// This function panics if there is any error encountered while constructing the required
/// configuration object from the environment. This is because a missing Kubernetes environment
/// is extremely terminal for which there truly is no alternative besides crashing.
pub async fn new<K>() -> Api<K>
where
    <K as Resource>::DynamicType: Default,
    K: k8s_openapi::Metadata<Ty = ObjectMeta>,
{
    new_with_namespace(crate::OCF_NAMESPACE).await
}

/// Returns a new Kubernetes client configured for the [OCF Namespace](crate::OCF_SYSTEM_NAMESPACE).
///
/// This function panics if there is any error encountered while constructing the required
/// configuration object from the environment. This is because a missing Kubernetes environment
/// is extremely terminal for which there truly is no alternative besides crashing.
pub async fn new_for_system<K>() -> Api<K>
where
    <K as Resource>::DynamicType: Default,
    K: k8s_openapi::Metadata<Ty = ObjectMeta>,
{
    new_with_namespace(crate::OCF_SYSTEM_NAMESPACE).await
}

/// Returns a new Kubernetes client configured for the given namespace.
///
/// This function panics if there is any error encountered while constructing the required
/// configuration object from the environment. This is because a missing Kubernetes environment
/// is extremely terminal for which there truly is no alternative besides crashing.
async fn new_with_namespace<K, N>(namespace: N) -> Api<K>
where
    <K as Resource>::DynamicType: Default,
    K: k8s_openapi::Metadata<Ty = ObjectMeta>,
    N: AsRef<str>,
{
    Api::namespaced(
        kube::Client::try_default()
            .await
            .map_err(ApiError::from)
            .unwrap(),
        namespace.as_ref(),
    )
}

#[async_trait]
pub trait Logs<T> {
    async fn stream_into<P: AsRef<Path> + Send>(&self, resource: &T, dst: P);
}

#[async_trait]
impl Logs<Pod> for Api<Pod> {
    async fn stream_into<P: AsRef<Path> + Send>(&self, resource: &Pod, dst: P) {
        let lp = &LogParams {
            container: None,
            follow: true,
            limit_bytes: None,
            pretty: false,
            previous: false,
            since_seconds: None,
            tail_lines: None,
            timestamps: false,
        };
        let stream = self
            .log_stream(resource.name().as_str(), &lp)
            .await
            .unwrap()
            .map(|err| match err {
                Err(err) => Err(StreamError::from(err)),
                Ok(buf) => Ok(buf),
            });
        let mut src = StreamReader::new(stream);
        let mut dst = BufWriter::new(tokio::fs::File::create(dst).await.unwrap());
        let _ = tokio::io::copy(&mut src, &mut dst).await;
    }
}

#[derive(Error, Debug)]
#[error("this is hard")]
struct StreamError {
    #[from]
    cause: kube::Error,
}

impl Into<std::io::Error> for StreamError {
    fn into(self) -> std::io::Error {
        std::io::Error::new(std::io::ErrorKind::BrokenPipe, self)
    }
}
