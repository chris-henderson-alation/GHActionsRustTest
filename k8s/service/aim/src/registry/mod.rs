pub mod containerd;
mod ecr;
mod minikube;

use crate::{aws, env};
pub use containerd::Image;
use error::*;
use result::Result;
use rocket::fs::TempFile;
use std::sync::Once;

static INIT: Once = Once::new();

/// An `Implementation` is an enumeration of all supported implementations of a container registry.
pub enum Implementation {
    /// ECR stands of the Elastic Container Registry and is a product of AWS. This is a valid
    /// production target implementation.
    Ecr,
    /// Minikube is a small, easy to use, Kubernetes packaging that is primarily used for
    /// local development. Minikube is NOT a valid production implementation! Minikube
    /// MUST be used for development and testing purposes ONLY.
    Minikube,
}

impl Implementation {
    /// Returns the running [Implementation](Implementation) that this cluster has be configured for.
    ///
    /// This function PANICS should the configured environment be for an unknown registry
    /// implementation. This is one more reason to call [configure](Implementation::configure)
    /// immediately upon program startup as that procedure exercises this procedure, which allows
    /// for a fail-fast experience that only affects SREs and not end users.
    pub fn which() -> Implementation {
        let implementation = env::implementation();
        match implementation.to_lowercase().as_str() {
            "ecr" => Implementation::Ecr,
            "minikube" => Implementation::Minikube,
            _ => panic!(
                "the IMPLEMENTATION environment variable was set to {}. \
            It can be one of either ECR or Minikube (case insensitive)",
                implementation
            ),
        }
    }

    /// Configures the running environment for the configured image registry implementation
    /// (as returned by the [IMPLEMENTATION](env::implementation) environment variable).
    ///
    /// This function PANICS should any failure occur.
    ///
    /// Consumers SHOULD call this function AT LEAST once during initialization in their main.
    /// While `configure` is indeed called at the beginning of all entry points into the registry
    /// module, since this function panics it is more desirable to see this panic occur upfront
    /// and on program startup (rather than waiting for a user to, say, install an image). In fact,
    /// `configure` exercises all code within the AIM that can panic, which makes it very desirable
    /// for eager execution.
    ///
    /// This function may be invoked as many times as once wishes. However, its body is guaranteed
    /// to only ever be executed exactly once.
    pub fn configure() {
        INIT.call_once(|| {
            futures::executor::block_on(async {
                match Implementation::which() {
                    Implementation::Minikube => {
                        warn!("This runtime is configured for use with Minikube. This should be for dev {}!", term_colors::red("ONLY"));
                    }
                    Implementation::Ecr => {
                        info!("Configuring this runtime for the {} (AWS ECR).", term_colors::bold("Elastic Container Registry"));
                        let key_id = env::aws_access_key_id();
                        let access_key = env::aws_secret_access_key();
                        let region = env::aws_region();
                        // Just assert that AWS_USERNAME is present.
                        let _ = env::aws_username();
                        aws!("configure", "set", "aws_access_key_id", &key_id)
                            .await
                            .unwrap();
                        aws!("configure", "set", "aws_secret_access_key", &access_key)
                            .await
                            .unwrap();
                        aws!("configure", "set", "region", &region).await.unwrap();
                    }
                };
            });
        })
    }
}

/// Imports the given tmp file as an image into the configure repository.
///
/// The image first undergoes a sanitization wherein it is imported
/// into `containerd` and retagged to an OCF normalized form before
/// being pushed to that target repository.
pub async fn import(image: TempFile<'_>) -> Result<Image> {
    Implementation::configure();
    containerd::import(image).await
}

/// Uninstalls the given tag from the configured repository. If no such
/// tag exists, then this procedure will silently succeed.
pub async fn uninstall(tag: String) -> Result<()> {
    Implementation::configure();
    match Implementation::which() {
        Implementation::Ecr => ecr::uninstall(tag).await,
        Implementation::Minikube => minikube::uninstall(tag).await,
    }
}

/// Returns a list of all images currently installed in the configured
/// repository. This list may be empty if the repository is empty.
pub async fn list() -> Result<Vec<Image>> {
    Implementation::configure();
    match Implementation::which() {
        Implementation::Ecr => ecr::list().await,
        Implementation::Minikube => minikube::list().await,
    }
}

/// Returns the `Image` associated with the given tag. If no such
/// tag exists, then an error of a [TagNotFound](TagNotFound) is returned.
/// This differs from the typical Rust convention of returning an `Option`
/// since callers of the top level API are expecting a non-existent tag
/// to result in an exception.
pub async fn get(tag: String) -> Result<Image> {
    Implementation::configure();
    let image = match Implementation::which() {
        Implementation::Ecr => ecr::get(&tag).await,
        Implementation::Minikube => minikube::get(&tag).await,
    }?;
    // Map a None result into an error for upstream clients.
    Ok(image.ok_or_else(|| TagNotFound {
        tag,
        registry: format!("{}/{}", env::registry(), env::repository()),
    })?)
}

#[derive(Error, AcmError, Kind, HttpCode, Debug)]
#[error("The OCF image tag '{tag}' does not exist in {registry}")]
#[code(Status::NotFound)]
pub struct TagNotFound {
    tag: String,
    registry: String,
}
