use crate::env;
use crate::env::Secret;
use crate::registry::Image;
use error::*;
use os::cmd;
use result::Result;
use serde;
use serde::Deserialize;
use serde_json;
use std::fmt::{Display, Formatter};

/// `aws` is a convenience macro for executing the [AWS CLI v2 Tooling](https://aws.amazon.com/cli/).
///
/// This macro returns a future of the output returned by [cmd](os::cmd) with the command `aws` pre-filled in.
///
/// ```ignore
/// let password = aws!("ecr", "get-login-password").await.unwrap();
/// ```
#[macro_export]
macro_rules! aws {
    ($($args:expr),*) => {
        cmd!("aws" $(,$args)*)
    }
}

/// `ecr` is a convenience macro for executing the
/// [AWS CLI v2 Tooling ECR Subcommand](https://awscli.amazonaws.com/v2/documentation/api/latest/reference/ecr/index.html).
///
/// This macro returns a future of the output returned by [cmd](os::cmd) with the command `aws ecr` pre-filled in.
///
/// ```ignore
/// let password = ecr!("get-login-password").await.unwrap();
/// ```
#[macro_export]
macro_rules! ecr {
    ($($args:expr),*) => {
        cmd!("aws", "ecr" $(,$args)*)
    }
}

/// An EcrUninstall is the deserialization target of the JSON returned
/// by the command `aws ecr batch-delete-image`.
///
/// For more information on this command, please see
/// [ecr::batch-delete-image](https://awscli.amazonaws.com/v2/documentation/api/latest/reference/ecr/batch-delete-image.html).
#[derive(Deserialize, Debug, Eq, PartialEq)]
struct EcrUninstall {
    #[serde(alias = "imageIds")]
    #[allow(unused)]
    image_ids: Vec<EcrImage>,
    failures: Vec<EcrUninstallFailure>,
}

/// Uninstalls the given tag from ECR. This is accomplished by running the
/// [ecr::batch-delete-image](https://awscli.amazonaws.com/v2/documentation/api/latest/reference/ecr/batch-delete-image.html)
/// command.
///
/// If multiple tags are assigned to the same digest, then only the tag submitted will be deleted
/// from ECR - the remaining tags are left in place. Upon deletion of the final tag that was
/// associated with a given digest, the digest will be deleted from ECR entirely.
///
/// If the provided tag was not found within ECR, then this procedure will silently succeed.
pub async fn uninstall(tag: String) -> Result<()> {
    let target = format!("imageTag={}", tag);
    let repository = env::repository();
    let result: EcrUninstall = serde_json::from_str(
        &ecr!(
            "batch-delete-image",
            "--repository-name",
            &repository,
            "--image-ids",
            &target
        )
        .await
        .map_err(|error| UninstallCommandError {
            error: format!("{}", error).into(),
        })?,
    )
    .map_err(|err| EcrUninstallSerdeError::from(err))?;
    match result.failures.as_slice() {
        [failure, ..] => match failure.failure_code.as_str() {
            // If there is no such image to delete then we consider that okay
            // since we were looking to delete it anyways.
            "ImageNotFound" => Ok(()),
            // Otherwise, something bad actually happened.
            _ => Err(EcrUninstallError::from(failure.clone()).into()),
        },
        _ => Ok(()),
    }
}

/// Returns the current ECR password associated with the globably configured account.
///
/// We say "current" because ECR is configured to rotate this password on a regular basis. As such
/// clients to this procedure SHOULD NOT call this function upfront and cache the result as the
/// result is unlikely to be valid for an extended period of time. Instead, clients should
/// call this procedure each time a password is required.
pub async fn get_password() -> Result<Secret> {
    Ok(ecr!("get-login-password")
        .await
        .map_err(|err| GetPasswordError::from(StringError::from(err)))?
        .into())
}

// Returning a `(Username, Secrete)` is clearer than returning a `(String, String)`.
// So...aliasing is useful here. Or you can make a struct. Or...
type Username = String;

/// Returns the username which is also retrievable via [env::aws_username](env::aws_username) as
/// well as the output of [get_password](get_password).
pub async fn get_credentials() -> Result<(Username, Secret)> {
    Ok((env::aws_username() as Username, get_password().await?))
}

/// An `EcrImage` is the deserialization target of the JSON returned by the
/// [AWS ECR ClI Tooling](https://awscli.amazonaws.com/v2/documentation/api/latest/reference/ecr/index.html#cli-aws-ecr).
#[derive(Deserialize, Debug, Clone, Eq, PartialEq)]
struct EcrImage {
    #[serde(alias = "imageDigest")]
    image_digest: String,
    #[serde(alias = "imageTag")]
    image_tag: String,
}

/// The [Display](std::fmt::Display) for an `EcrImage` is the fully qualified reference
/// (that is, `<registry>/<repository>:<tag>`) followed by the digest
/// of the image.
///
/// This format should typically not be used for anything other than logging and
/// strings displayed to the end user.
impl Display for EcrImage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let reference = format!(
            "{}/{}:{}, {}",
            env::registry(),
            env::repository(),
            self.image_tag,
            self.image_digest
        );
        f.write_str(&reference)
    }
}

/// Converts ECR's representation of a `(tag, digest)` pairing into our own representation.
impl Into<Image> for EcrImage {
    fn into(self) -> Image {
        Image {
            tag: self.image_tag,
            digest: self.image_digest,
        }
    }
}

#[derive(Deserialize, Debug, Eq, PartialEq)]
struct EcrListImages {
    #[serde(alias = "imageIds")]
    image_ids: Vec<EcrImage>,
}

/// Lists all images (if any) currently in the configured ECR repository. This is accomplished
/// by running the [list-images](https://awscli.amazonaws.com/v2/documentation/api/latest/reference/ecr/list-images.html)
/// command.
pub async fn list() -> Result<Vec<Image>> {
    let repository = env::repository();
    let images: EcrListImages = serde_json::from_str(
        &ecr!(
            "list-images",
            "--no-paginate",
            "--repository-name",
            &repository
        )
        .await?,
    )
    .map_err(|err| EcrImageSerdeError::from(err))?;
    Ok(images.image_ids.into_iter().map(EcrImage::into).collect())
}

/// Retrieves the given tag from the configured ECR repository. If no such
/// tag exists, then `Ok(None)` is returned.
pub async fn get<T: AsRef<str>>(tag: T) -> Result<Option<Image>> {
    Ok(list()
        .await?
        .into_iter()
        .find(|image| image.tag.eq(tag.as_ref())))
}

#[derive(Error, AcmError, Kind, HttpCode, Debug)]
#[code(Status::InternalServerError)]
#[error(
    "A failure occurred while deserializing the JSON representation for the response returned by \
AWS ECR's deletion endpoint. We expected a data structure similar to that documented in \
https://awscli.amazonaws.com/v2/documentation/api/latest/reference/ecr/batch-delete-image.html"
)]
struct EcrUninstallSerdeError {
    #[from]
    error: serde_json::Error,
}

#[derive(Error, AcmError, Kind, HttpCode, Debug)]
#[code(Status::InternalServerError)]
#[error(
    "A failure occurred while deserializing the JSON representation for the response returned by \
AWS ECR's list-images endpoint. We expected a data structure similar to that documented in \
https://awscli.amazonaws.com/v2/documentation/api/latest/reference/ecr/list-images.html"
)]
struct EcrImageSerdeError {
    #[from]
    error: serde_json::Error,
}

#[derive(Error, AcmError, Kind, HttpCode, Deserialize, Debug, Eq, PartialEq, Clone)]
#[code(Status::BadRequest)]
#[error(
    "ECR reported the failure code '{failure_code}' when attempting to uninstall '{image_id}'. \
The given reason was '{failure_reason}'."
)]
struct EcrUninstallFailure {
    #[serde(alias = "imageId")]
    image_id: EcrFailedImageUninstall,
    #[serde(alias = "failureCode")]
    failure_code: String,
    #[serde(alias = "failureReason")]
    failure_reason: String,
}

#[derive(Deserialize, Debug, Eq, PartialEq, Clone)]
struct EcrFailedImageUninstall {
    #[serde(alias = "imageTag")]
    image_tag: String,
}

impl Display for EcrFailedImageUninstall {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.image_tag.fmt(f)
    }
}

#[derive(Error, AcmError, Kind, HttpCode, Debug)]
#[code(Status::BadRequest)]
#[error("A failure occurred while attempting to complete the deletion in the Elastic Container Registry. \
If this is a networking failure, then perhaps reattempting at a later time may succeed.")]
struct EcrUninstallError {
    #[from]
    cause: EcrUninstallFailure,
}

#[derive(Error, AcmError, Kind, HttpCode, Debug)]
#[code(Status::InternalServerError)]
#[error(
    "A raw error was returned from the AWS Elastic Container Registry API. This is usually \
indicative of an extreme failure case, such as a missing repository or expired/incorrect \
credentials. Do note, however, that ECR does make some interesting policy decisions with regard \
to what this error string is actually reporting. For example, a non-existent repository may \
actually be reported as an authorization error."
)]
struct UninstallCommandError {
    #[source]
    error: StringError,
}

#[derive(Error, AcmError, Kind, HttpCode, Debug)]
#[code(Status::InternalServerError)]
#[error(
    "An error occurred while retrieving a password to use for the AWS Elastic Container \
Registry used by this Alation cluster. If this was a networking error, then perhaps the issue \
will resolve itself over time and it may be reattempted in the future. Otherwise, please contact \
Alation's site reliability engineering with the contents of this error."
)]
struct GetPasswordError {
    #[from]
    error: StringError,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_ecr_image() {
        // Picked up from https://awscli.amazonaws.com/v2/documentation/api/latest/reference/ecr/list-images.html
        let response = r#"{
    "imageIds": [
        {
            "imageDigest": "sha256:99c6fb4377e9a420a1eb3b410a951c9f464eff3b7dbc76c65e434e39b94b6570",
            "imageTag": "v1.13.8"
        },
        {
            "imageDigest": "sha256:99c6fb4377e9a420a1eb3b410a951c9f464eff3b7dbc76c65e434e39b94b6570",
            "imageTag": "v1.13.7"
        },
        {
            "imageDigest": "sha256:4a1c6567c38904384ebc64e35b7eeddd8451110c299e3368d2210066487d97e5",
            "imageTag": "v1.13.6"
        }
    ]
}"#;
        let got: EcrListImages = serde_json::from_str(response).unwrap();
        let want = EcrListImages {
            image_ids: vec![
                EcrImage {
                    image_digest:
                        "sha256:99c6fb4377e9a420a1eb3b410a951c9f464eff3b7dbc76c65e434e39b94b6570"
                            .to_string(),
                    image_tag: "v1.13.8".to_string(),
                },
                EcrImage {
                    image_digest:
                        "sha256:99c6fb4377e9a420a1eb3b410a951c9f464eff3b7dbc76c65e434e39b94b6570"
                            .to_string(),
                    image_tag: "v1.13.7".to_string(),
                },
                EcrImage {
                    image_digest:
                        "sha256:4a1c6567c38904384ebc64e35b7eeddd8451110c299e3368d2210066487d97e5"
                            .to_string(),
                    image_tag: "v1.13.6".to_string(),
                },
            ],
        };
        assert_eq!(got, want);
    }

    #[test]
    fn deserialize_uninstall_image() {
        // Picked up from https://awscli.amazonaws.com/v2/documentation/api/latest/reference/ecr/batch-delete-image.html
        let response = r#"{
    "failures": [],
    "imageIds": [
        {
            "imageTag": "precise",
            "imageDigest": "sha256:19665f1e6d1e504117a1743c0a3d3753086354a38375961f2e665416ef4b1b2f"
        }
    ]
}"#;
        let got: EcrUninstall = serde_json::from_str(response).unwrap();
        let want = EcrUninstall {
            image_ids: vec![EcrImage {
                image_tag: "precise".to_string(),
                image_digest:
                    "sha256:19665f1e6d1e504117a1743c0a3d3753086354a38375961f2e665416ef4b1b2f"
                        .to_string(),
            }],
            failures: vec![],
        };
        assert_eq!(got, want);
    }

    #[test]
    fn deserialize_uninstall_image_error() {
        let response = r#"{
    "imageIds": [],
    "failures": [
        {
            "imageId": {
                "imageTag": "precise"
            },
            "failureCode": "ImageNotFound",
            "failureReason": "Requested image not found"
        }
    ]
}
"#;
        let got: EcrUninstall = serde_json::from_str(response).unwrap();
        let want = EcrUninstall {
            image_ids: vec![],
            failures: vec![EcrUninstallFailure {
                image_id: EcrFailedImageUninstall {
                    image_tag: "precise".to_string(),
                },
                failure_code: "ImageNotFound".to_string(),
                failure_reason: "Requested image not found".to_string(),
            }],
        };
        assert_eq!(got, want);
    }
}
