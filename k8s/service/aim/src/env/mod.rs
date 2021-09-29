use std::env::VarError;
use std::ffi::OsStr;
use std::fmt::{Debug, Display, Formatter};

/// The registry configured under the `REGISTRY` environment variable. If no such environment
/// variable is set, then this function defaults to `registry.kube-system` (which is the
/// registry used for local Minikube development).
///
/// The `REGISTRY` environment variable MUST be a valid and reachable DNS entry for an OCI
/// compliant registry. It MUST NOT include the protocol nor target repository.
///
/// A valid example may be `248135293344.dkr.ecr.us-east-2.amazonaws.com`.
pub fn registry() -> String {
    std::env::var("REGISTRY").unwrap_or_else(|_| String::from("registry.kube-system"))
}

/// The repository configured under the `REPOSITORY` environment variable. If no such environment
/// variable is set, then this function defaults to `ocf` (which is the repository used
/// for local Minikube development).
pub fn repository() -> String {
    std::env::var("REPOSITORY").unwrap_or_else(|_| String::from("ocf"))
}

/// The registry implementation configured under the `IMPLEMENTATION` environment variable. If no
/// such environment variable is set, then this function defaults to `Minikube` (which is the
/// implementation used for local Minikube development).
///
/// Valid implementations are:
/// * `ECR`
/// * `Minikube` (for development and testing ONLY!)
pub fn implementation() -> String {
    std::env::var("IMPLEMENTATION").unwrap_or_else(|_| String::from("Minikube"))
}

/// The AWS region configured under the `AWS_REGION` environment variable. This is the AWS region
/// in which the configured [registry](registry) is running. For more information regarding
/// AWS regions, please see [Regions and Availability Zones](https://aws.amazon.com/about-aws/global-infrastructure/regions_az/).
///
/// There is NO default associated with this environment variable. If this function is
/// called without the environment variable being set then this function will PANIC!
///
/// The `AWS_REGION` environment variable is MANDATORY when the configured
/// (implementation)[implementation] is `ECR`.
pub fn aws_region() -> String {
    std::env::var("AWS_REGION")
        .and_then(map_empty_to_error)
        .expect(
            "The AWS_REGION environment variable is mandatory when using the ECR implementation",
        )
}

/// The AWS access key ID configured under the `AWS_ACCESS_KEY_ID` environment variable. This
/// is the AWS access key ID used to make API calls for the configured [registry](registry).
/// For more information regarding AWS programmatic credentials, please see
/// [Understanding and getting your AWS credentials - Programmatic access](https://docs.aws.amazon.com/general/latest/gr/aws-sec-cred-types.html#access-keys-and-secret-access-keys).
///
/// There is NO default associated with this environment variable. If this function is
/// called without the environment variable being set then this function will PANIC!
///
/// The `AWS_ACCESS_KEY_ID` environment variable is MANDATORY when the configured
/// (implementation)[implementation] is `ECR`.
pub fn aws_access_key_id() -> String {
    std::env::var("AWS_ACCESS_KEY_ID").and_then(map_empty_to_error).expect(
        "The AWS_ACCESS_KEY_ID environment variable is mandatory when using the ECR implementation",
    )
}

/// The AWS secret access key configured under the `AWS_SECRET_ACCESS_KEY` environment variable.
/// This is the AWS secret access key used to make API calls for the configured [registry](registry).
/// For more information regarding AWS programmatic credentials, please see
/// [Understanding and getting your AWS credentials - Programmatic access](https://docs.aws.amazon.com/general/latest/gr/aws-sec-cred-types.html#access-keys-and-secret-access-keys).
///
/// There is NO default associated with this environment variable. If this function is
/// called without the environment variable being set then this function will PANIC!
///
/// The `AWS_SECRET_ACCESS_KEY` environment variable is MANDATORY when the configured
/// (implementation)[implementation] is `ECR`.
pub fn aws_secret_access_key() -> Secret {
    std::env::var("AWS_SECRET_ACCESS_KEY").and_then(map_empty_to_error).expect(
        "The AWS_SECRET_ACCESS_KEY environment variable is mandatory when using the ECR implementation",
    ).into()
}

/// The AWS IAM user configured under the `AWS_USERNAME` environment variable.
/// This is the AWS IAM user used to make API calls for the configured [registry](registry).
/// For more information regarding AWS programmatic credentials, please see
/// [Understanding and getting your AWS credentials - Programmatic access](https://docs.aws.amazon.com/general/latest/gr/aws-sec-cred-types.html#access-keys-and-secret-access-keys).
///
/// There is NO default associated with this environment variable. If this function is
/// called without the environment variable being set then this function will PANIC!
///
/// The `AWS_USERNAME` environment variable is MANDATORY when the configured
/// (implementation)[implementation] is `ECR`.
pub fn aws_username() -> String {
    std::env::var("AWS_USERNAME")
        .and_then(map_empty_to_error)
        .expect(
            "The AWS_USERNAME environment variable is mandatory when using the ECR implementation",
        )
}

/// If an environment variable is technically present, albeit empty, then we would like to
/// take that to mean that it doesn't actually exist.
fn map_empty_to_error(var: String) -> std::result::Result<String, VarError> {
    if var.is_empty() {
        Err(VarError::NotPresent)
    } else {
        Ok(var)
    }
}

/// A `Secret` obfuscates an underlying string from being accidentally printed to any logs.
///
/// Any attempt to format a `Secret` using the either the [Display](Display)("{}") or [Debug](Debug)
/// ("{:?}") directives will result in the string "<REDACTED>" rather than the underlying secret.
///
/// Original secret may be retrieved by either requesting a reference to a [String](String)/[str](str)
/// or by explicitly calling [raw_secret](Secret::raw_secret).  
///
/// ```
/// let password = Secret::from("please don't log this");
/// let log_entry = format!("my password is {}!", password);
/// assert_eq!("my password is <REDACTED>!", log_entry);
/// ```
pub struct Secret {
    secret: String,
}

impl Secret {
    pub fn raw_secret(&self) -> &str {
        self.as_ref()
    }
}

impl Display for Secret {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("<REDACTED>")
    }
}

impl Debug for Secret {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("<REDACTED>")
    }
}

impl From<String> for Secret {
    fn from(secret: String) -> Self {
        Self { secret }
    }
}

impl From<&String> for Secret {
    fn from(secret: &String) -> Self {
        Self {
            secret: secret.clone(),
        }
    }
}

impl From<&str> for Secret {
    fn from(secret: &str) -> Self {
        Self::from(secret.to_string())
    }
}

impl AsRef<str> for Secret {
    fn as_ref(&self) -> &str {
        self.secret.as_str()
    }
}

impl AsRef<String> for Secret {
    fn as_ref(&self) -> &String {
        &self.secret
    }
}

impl AsRef<OsStr> for Secret {
    fn as_ref(&self) -> &OsStr {
        self.secret.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_display() {
        let password = Secret::from("please don't log this");
        let log_entry = format!("my password is {}!", password);
        assert_eq!("my password is <REDACTED>!", log_entry);
    }

    #[test]
    fn test_secret_debug() {
        let password = Secret::from("please don't log this");
        let log_entry = format!("my password is {:?}!", password);
        assert_eq!("my password is <REDACTED>!", log_entry);
    }
}
