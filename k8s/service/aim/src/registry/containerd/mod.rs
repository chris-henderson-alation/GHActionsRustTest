mod import;
mod namespace;
mod push;
pub mod retag;
mod tmp_image;
mod workflow;

use crate::registry::containerd::namespace::Namespace;
use crate::registry::containerd::tmp_image::TmpImage;
use crate::registry::containerd::workflow::WorkFlow;
use kind::Kind;
use result::Result;
use rocket::fs::TempFile;
use serde::Serialize;

/// `ctr` is a convenience macro for executing the [ctr command](https://github.com/containerd/containerd/tree/main/cmd/ctr)
/// which is a CLI tool for interacting with containerd.
///
/// This macro returns a future of the output returned by [cmd](os::cmd) with the command `ctr` pre-filled in.
///
/// ```ignore
/// ctr!("images", "ls").await.unwrap();
/// ```
#[macro_export]
macro_rules! ctr {
    ($($args:expr),*) => {
        cmd!("ctr" $(,$args)*)
    }
}

/// An Image is a pairing of a tag and a digest and is intended to be the final representation
/// of an image that is sent back upstream to calling clients.
#[derive(Serialize, Debug, Kind)]
pub struct Image {
    pub tag: String,
    pub digest: String,
}

/// This conversion consumes the [TmpImage](TmpImage) that was within containerd during
/// sanitization. Doing so triggers [TmpImage::drop](TmpImage::drop) which initiates destruction
/// of the temporary image within containerd.
impl From<TmpImage<'_>> for Image {
    fn from(image: TmpImage<'_>) -> Self {
        Image {
            tag: image.tag.clone(),
            digest: image.digest.clone(),
        }
    }
}

/// This procedure imports the given OCI compliant image into the configure registry using
/// containerd as the intermediate for retagging and pushing.
///
/// The pipeline for this procedure is as follows:
///
/// 1. Import the file as is into containerd under a unique namespace.
/// 2. Retag the imported image with a new <[registry](crate::env::registry)>/<[repository](crate::env::repository)>:<[tag](names::rfc1035_label())>.
/// 3. Push the newly tagged image into the remote registry.
pub async fn import(image: TempFile<'_>) -> Result<Image> {
    let namespace = Namespace::new();
    let image = WorkFlow::new_workflow(&namespace)
        .import(image)
        .await?
        .retag()
        .await?
        .push()
        .await?;
    Ok(image)
}
