use crate::ctr;
use backoff::backoff::Backoff;
use std::ffi::OsStr;
use std::fmt::{Display, Formatter};

/// A Namespace is a randomly generated (UUID) containerd namespace
/// that is used for conducting the import workflow. All steps of the workflow
/// ([import](super::import::Import), [retag](super::retag::Retag), and [push](super::push::Push))
/// are all conducted under this namespace.
///
/// Having a unique namespace gives us some measure of guarantee as to what the behavior
/// and output of the `ctr` command will be at various steps. Notably, after importing an
/// image into containerd under this namespace, containerd is guaranteed to return exactly
/// one object from `ctr -n <NAMESPACE> images ls`. This makes parsing the CLI output far
/// easier than it would have been otherwise.
///
/// Additionally, it protects us from having to understand what would happen if an attempt was
/// made to install two or more of the same image at the same time.
pub struct Namespace {
    pub namespace: String,
}

impl Namespace {
    pub fn new() -> Namespace {
        Namespace {
            namespace: names::uuid(),
        }
    }
}

/// The [drop](Drop) implementation for a namespace guarantees that it is always destroyed
/// upon the exit of the import workflow.
///
/// The exception to this guarantee might be if there is a bug in [TmpImage::drop](super::tmp_image::TmpImage::drop)
/// which prevents those temporary images from being destroyed. In this case, the namespace can
/// never be successfully destroyed as containerd refuses to do so on a non-empty namespace.
///
/// containerd itself has some race conditions within it. That is, we may submit a deletion request
/// for an image, followed by a deletion request for its containing namespace. However, under the
/// hood, containerd has not yet finished deletion of the the aforementioned temporary image, resulting
/// in a failure when deleting the namespace. This why this drop method is ran in the background
/// in order to eventually complete, and with an exponential backoff in order to automatically retry.
///
/// If a namespace does become orphaned for whatever reason then an error is logged. In order to
/// recover from this error (that is, force a cleanup of the namespace) one need only restart
/// the AIM's pod.
impl Drop for Namespace {
    fn drop(&mut self) {
        let namespace = self.namespace.clone();
        let namespace_display = term_colors::cyan(namespace.clone());
        tokio::spawn(async move {
            debug!(
                "Beginning destruction of temporary namespace {}",
                namespace_display
            );
            let mut backoff = backoff::ExponentialBackoff::default();
            loop {
                let result = ctr!("namespace", "remove", &namespace).await;
                let pause = backoff.next_backoff();
                match (result, pause) {
                    (Err(err), Some(pause)) => {
                        trace!(
                            "Failed to run command to destroy tmp names[ace {}, '{}'",
                            namespace_display,
                            err
                        );
                        tokio::time::sleep(pause).await;
                    }
                    (Err(err), None) => {
                        error!(
                            "{}, stopping reattempts so the namespace {} may be orphaned now. \
                            These orphans can be cleaned up simply by restarted the aim's pod.",
                            err, namespace_display
                        );
                        return;
                    }
                    (Ok(_), _) => {
                        debug!(
                            "Temporary namespace {} successfully deleted",
                            namespace_display
                        );
                        return;
                    }
                };
            }
        });
    }
}

impl AsRef<OsStr> for Namespace {
    fn as_ref(&self) -> &OsStr {
        self.namespace.as_ref()
    }
}

impl AsRef<str> for Namespace {
    fn as_ref(&self) -> &str {
        self.namespace.as_ref()
    }
}

impl Display for Namespace {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.namespace.fmt(f)
    }
}
