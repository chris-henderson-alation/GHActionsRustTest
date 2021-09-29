use error::*;
use k8s_openapi::api::core::v1::{
    ContainerState, ContainerStateTerminated, ContainerStateWaiting, Pod, PodStatus,
};
use result::Result;
use serde_json;

#[derive(AcmError, Error, Kind, HttpCode, Debug)]
#[error(
    "Failed to serialize a Kubernetes pod resource with the name '{name}' \
    and image reference '{reference}'. This is very peculiar, and it may \
    work if you simply run it again, although this error should be reported \
    to Alation so that we can make sure it never happens again."
)]
#[code(Status::InternalServerError)]
pub struct PodSerializationError {
    name: String,
    reference: String,
    #[source]
    source: serde_json::Error,
}

pub fn new<R: AsRef<str>, N: AsRef<str>>(reference: R, name: N) -> Result<Pod> {
    let reference = reference.as_ref();
    let name = names::rfc1123_subdomain(name);
    let pod: Pod = serde_json::from_value(serde_json::json!({
       "apiVersion":"v1",
       "kind":"Pod",
       "metadata":{
          "name": name,
          "namespace": super::OCF_NAMESPACE
       },
       "spec":{
          "containers":[
             {
                "name": name,
                "image": reference,
                "env":[
                   {
                      "name":"PORT",
                      "value":"8080"
                   }
                ],
                "restartPolicy":"Never",
                "imagePullPolicy":"IfNotPresent",
                "ports":[
                   {
                      "containerPort":8080,
                      "protocol":"TCP"
                   }
                ]
             }
          ]
       }
    }))
    .map_err(|source| PodSerializationError {
        name: name.to_string(),
        reference: reference.to_string(),
        source,
    })?;
    Ok(pod)
}

/// PodExt is an extension trait used to answer common questions about pods.
pub trait PodExt {
    fn dns(&self) -> Result<String>;
    fn port(&self) -> Result<i32>;
    fn address(&self) -> Result<String>;
    fn running(&self) -> bool;
    fn crashed(&self) -> bool;
    fn terminated(&self) -> bool;
    fn terminated_reason(&self) -> Option<String>;
    fn terminated_message(&self) -> Option<String>;
    fn was_err_image_pull(&self) -> bool;
    fn err_image_pull(&self) -> Result<()>;
}

impl PodExt for Pod {
    fn dns(&self) -> Result<String> {
        let subdomain = self
            .status
            .as_ref()
            .ok_or_else(|| PodHasNoStatus {
                op: "retrieving its cluster DNS entry".to_string(),
            })?
            .pod_ip
            .as_ref()
            .ok_or_else(|| PodHasNoIp {
                op: "retrieving its cluster DNS entry".to_string(),
            })?
            .replace('.', "-");
        let domain = self
            .metadata
            .namespace
            .as_ref()
            .ok_or_else(|| PodHasNoNamespace {
                op: "retrieving its cluster DNS entry".to_string(),
            })?;
        Ok(format!("{}.{}.pod", subdomain, domain))
    }

    fn port(&self) -> Result<i32> {
        Ok(self
            .spec
            .as_ref()
            .ok_or_else(|| PodHasNoSpec {
                op: "retrieving its listening port number".to_string(),
            })?
            .containers
            .get(0)
            .as_ref()
            .ok_or_else(|| PodHasNoContainers {
                op: "retrieving its listening port number".to_string(),
            })?
            .ports
            .as_ref()
            .ok_or_else(|| ContainerHasNoPorts {
                op: "retrieving its listening port number".to_string(),
            })?
            .get(0)
            .as_ref()
            .ok_or_else(|| ContainerHasNoPorts {
                op: "retrieving its listening port number".to_string(),
            })?
            .container_port)
    }

    fn address(&self) -> Result<String> {
        Ok(format!("{}:{}", self.dns()?, self.port()?))
    }

    fn running(&self) -> bool {
        let default_state = ContainerState::default();
        let default_status = PodStatus::default();
        let default_statuses = vec![];
        self.status
            .as_ref()
            .unwrap_or(&default_status)
            .container_statuses
            .as_ref()
            .unwrap_or(&default_statuses)
            .iter()
            .any(|status| {
                let state = status.state.as_ref().unwrap_or(&default_state);
                // Either we have begun execution and can being logging
                // or the darned thing instagibbed itself so we should
                // go pick up its logs.
                state.running.is_some()
            })
    }

    fn terminated(&self) -> bool {
        let default_state = ContainerState::default();
        let default_status = PodStatus::default();
        let default_statuses = vec![];
        self.status
            .as_ref()
            .unwrap_or(&default_status)
            .container_statuses
            .as_ref()
            .unwrap_or(&default_statuses)
            .iter()
            .any(|status| {
                let state = status.state.as_ref().unwrap_or(&default_state);
                // Either we have begun execution and can being logging
                // or the darned thing instagibbed itself so we should
                // go pick up its logs.
                state.terminated.is_some()
            })
    }

    fn was_err_image_pull(&self) -> bool {
        let default_state = ContainerState::default();
        let default_status = PodStatus::default();
        let default_statuses = vec![];
        let status = self
            .status
            .as_ref()
            .unwrap_or(&default_status)
            .container_statuses
            .as_ref()
            .unwrap_or(&default_statuses)
            .iter()
            .find(|status| {
                let state = status.state.as_ref().unwrap_or(&default_state);
                // Either we have begun execution and can being logging
                // or the darned thing instagibbed itself so we should
                // go pick up its logs.
                match state.waiting.as_ref() {
                    Some(ContainerStateWaiting {
                        reason: Some(reason),
                        ..
                    }) if reason.eq("ErrImagePull") => true,
                    _ => false,
                }
            });
        status.is_some()
    }

    fn err_image_pull(&self) -> Result<()> {
        let default_state = ContainerState::default();
        let default_status = PodStatus::default();
        let default_statuses = vec![];
        let status = self
            .status
            .as_ref()
            .unwrap_or(&default_status)
            .container_statuses
            .as_ref()
            .unwrap_or(&default_statuses)
            .iter()
            .find(|status| {
                let state = status.state.as_ref().unwrap_or(&default_state);
                // Either we have begun execution and can being logging
                // or the darned thing instagibbed itself so we should
                // go pick up its logs.
                match state.waiting.as_ref() {
                    Some(ContainerStateWaiting {
                        reason: Some(reason),
                        ..
                    }) if reason.eq("ErrImagePull") => true,
                    _ => false,
                }
            });
        if let Some(problem) = status {
            Err(ErrImagePull {
                message: ErrImagePullCause {
                    // NOTE: We can unwrap here only because we were so careful above.
                    // If you change anything about the above then you MUST reaffirm
                    // that these unwraps are safe.
                    message: problem
                        .state
                        .as_ref()
                        .unwrap()
                        .waiting
                        .as_ref()
                        .unwrap()
                        .message
                        .as_ref()
                        .unwrap()
                        .into(),
                },
            }
            .into())
        } else {
            Ok(())
        }
    }
    fn crashed(&self) -> bool {
        let default_state = ContainerState::default();
        let default_status = PodStatus::default();
        let default_statuses = vec![];
        let status = self
            .status
            .as_ref()
            .unwrap_or(&default_status)
            .container_statuses
            .as_ref()
            .unwrap_or(&default_statuses)
            .iter()
            .find(|status| {
                let state = status.state.as_ref().unwrap_or(&default_state);
                // Either we have begun execution and can being logging
                // or the darned thing instagibbed itself so we should
                // go pick up its logs.
                match state.waiting.as_ref() {
                    Some(ContainerStateWaiting {
                        reason: Some(reason),
                        ..
                    }) if reason.eq("CrashLoopBackOff") => true,
                    _ => false,
                }
            });
        status.is_some()
    }

    fn terminated_reason(&self) -> Option<String> {
        let default_state = ContainerState::default();
        let default_status = PodStatus::default();
        let default_statuses = vec![];
        let mut status: Vec<Option<String>> = self
            .status
            .as_ref()
            .unwrap_or(&default_status)
            .container_statuses
            .as_ref()
            .unwrap_or(&default_statuses)
            .iter()
            .map(|status| {
                let state = status.state.as_ref().unwrap_or(&default_state);
                match state {
                    ContainerState {
                        terminated:
                            Some(ContainerStateTerminated {
                                reason: Some(reason),
                                ..
                            }),
                        ..
                    } => Some(reason.clone()),
                    _ => None,
                }
            })
            .collect();
        status.pop().unwrap_or(None)
    }

    fn terminated_message(&self) -> Option<String> {
        let default_state = ContainerState::default();
        let default_status = PodStatus::default();
        let default_statuses = vec![];
        let mut status: Vec<Option<String>> = self
            .status
            .as_ref()
            .unwrap_or(&default_status)
            .container_statuses
            .as_ref()
            .unwrap_or(&default_statuses)
            .iter()
            .map(|status| {
                let state = status.state.as_ref().unwrap_or(&default_state);
                match state {
                    ContainerState {
                        terminated:
                            Some(ContainerStateTerminated {
                                message: Some(message),
                                ..
                            }),
                        ..
                    } => Some(message.clone()),
                    _ => None,
                }
            })
            .collect();
        status.pop().unwrap_or(None)
    }
}

#[derive(Error, AcmError, HttpCode, Kind, Debug)]
#[error(
    "The image for the connector failed to get pulled from the configured image registry. \
Perhaps the image doesn't exist or the connection to the registry couldn't be established?"
)]
#[code(error::Status::NotFound)]
struct ErrImagePull {
    #[source]
    message: ErrImagePullCause,
}

#[derive(Error, AcmError, HttpCode, Kind, Debug)]
#[error("{message}")]
#[code(error::Status::NotFound)]
struct ErrImagePullCause {
    message: String,
}

#[derive(Error, AcmError, HttpCode, Kind, Debug)]
#[code(error::Status::InternalServerError)]
#[error(
    "An attempt was made to retrieve the status field of a pod object while {op}, however the \
object had no status field. This was likely a premature call to a pod object that had not yet \
been provisioned in Kubernetes."
)]
struct PodHasNoStatus {
    op: String,
}

#[derive(Error, AcmError, HttpCode, Kind, Debug)]
#[code(error::Status::InternalServerError)]
#[error(
    "An attempt was made to retrieve the pod IP field of a pod object while {op}, however the \
object had no IP. This was likely a premature call to a pod object that had not yet \
been provisioned in Kubernetes."
)]
struct PodHasNoIp {
    op: String,
}

#[derive(Error, AcmError, HttpCode, Kind, Debug)]
#[code(error::Status::InternalServerError)]
#[error(
    "An attempt was made to retrieve the namespace of a pod object while {op}, however the \
object had no namespace associated with it. This was likely a premature call to a pod object \
that had not yet been provisioned in Kubernetes."
)]
struct PodHasNoNamespace {
    op: String,
}

#[derive(Error, AcmError, HttpCode, Kind, Debug)]
#[code(error::Status::InternalServerError)]
#[error(
    "An attempt was made to retrieve the spec of a pod object while {op}, however the \
object had no spec associated with it. This was likely a premature call to a pod object \
that had not yet been provisioned in Kubernetes."
)]
struct PodHasNoSpec {
    op: String,
}

#[derive(Error, AcmError, HttpCode, Kind, Debug)]
#[code(error::Status::InternalServerError)]
#[error(
"An attempt was made to retrieve at least one container associated with a pod object while \
{op}, however the object had no containers associated with it. This was likely a premature call to a \
pod object that had not yet been provisioned in Kubernetes."
)]
struct PodHasNoContainers {
    op: String,
}

#[derive(Error, AcmError, HttpCode, Kind, Debug)]
#[code(error::Status::InternalServerError)]
#[error(
"An attempt was made to retrieve at least one listening port associated with a container object while \
{op}, however the object had no listening ports associated with it. This was likely a premature call to a \
pod object that had not yet been provisioned in Kubernetes."
)]
struct ContainerHasNoPorts {
    op: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty() {
        new("".to_string(), "asdas").unwrap();
    }

    #[test]
    fn not_rfc1123_compliant_name() {
        new("not a bloody chance".to_string(), "asdas").unwrap();
    }
}
