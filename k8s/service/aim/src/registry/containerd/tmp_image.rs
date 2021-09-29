use super::namespace::Namespace;
use crate::ctr;
use backoff::backoff::Backoff;
use std::ffi::OsStr;
use std::fmt::{Display, Formatter};
use term_colors;

/// A `TmpImage` represents our target image installation during an intermediate stage of the importation workflow.
///
/// During the [import](super::import::Import) step it will take on the reference, tag, and digest
/// of the original image. Ownership of the resulting `TmpImage` is then passed onto [retag](super::retag::Retag)
/// which constructs a new `TmpImage` with the newly generated reference and tag (with the
/// digest and namespace staying the same) and [drops](TmpImage::drop) its reference to the original
/// `TmpImage`, thus destroying it in containerd. The retagged `TmpImage` is then passed onto
/// [push](super::push::Push) which pushes the image into our remote repository. Finally, the second
/// `TmpImage` is dropped, leading to its destruction in containerd as well.
pub struct TmpImage<'a> {
    pub reference: String,
    pub tag: String,
    pub digest: String,
    pub namespace: &'a Namespace,
}

/// The [drop](Drop) implementation for a `TmpImage` guarantees that it is always destroyed
/// upon the exit of the import workflow.
///
/// containerd itself has some race conditions within it. That is, we may submit a deletion request
/// for an image, followed by a deletion request for its containing namespace. However, under the
/// hood, containerd has not yet finished deletion of the the aforementioned temporary image, resulting
/// in a failure when deleting the namespace. This why this drop method is ran in the background
/// in order to eventually complete, and with an exponential backoff in order to automatically retry.
///
/// While the above issue is more of a problem for [Namespaces](super::namespace::Namespace), we
/// honor it here as well by running an exponential backoff in a background coroutine.
impl Drop for TmpImage<'_> {
    fn drop(&mut self) {
        let namespace = self.namespace.namespace.clone();
        let reference = self.reference.clone();
        let image_display = term_colors::cyan(format!("{}:{}", namespace, reference));
        tokio::spawn(async move {
            debug!("Beginning destruction of temporary image {}", image_display);
            let mut backoff = backoff::ExponentialBackoff::default();
            loop {
                let result = ctr!("-n", &namespace, "images", "remove", &reference).await;
                let pause = backoff.next_backoff();
                match (result, pause) {
                    (Err(err), Some(pause)) => {
                        trace!(
                            "Failed to run command to destroy tmp image {}, '{}'",
                            image_display,
                            err
                        );
                        tokio::time::sleep(pause).await;
                    }
                    (Err(err), None) => {
                        error!(
                            "{}, stopping reattempts so the image {} may be orphaned now. \
                        These orphans can be cleaned up simply by restarted the aim's pod.",
                            err, image_display
                        );
                        return;
                    }
                    (Ok(_), _) => {
                        debug!("Temporary image {} successfully deleted", image_display);
                        return;
                    }
                }
            }
        });
    }
}

impl AsRef<OsStr> for TmpImage<'_> {
    fn as_ref(&self) -> &OsStr {
        self.reference.as_ref()
    }
}

impl AsRef<str> for TmpImage<'_> {
    fn as_ref(&self) -> &str {
        self.reference.as_ref()
    }
}

impl Display for TmpImage<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.reference.fmt(f)
    }
}
