use crate::registry::containerd::push::Push;
use crate::registry::containerd::tmp_image::TmpImage;
use crate::{ctr, env};
use result::Result;

/// The Retag step takes ownership of a [TmpImage](TmpImage) and offers
/// a single method...[Retag::retag](Retag::retag).
pub struct Retag<'a> {
    pub image: TmpImage<'a>,
}

impl<'a> Retag<'a> {
    /// Retags the aggregated [TmpImage](TmpImage) into one that is
    /// appropriate for consumption by the configured OCF connector
    /// repository.
    ///
    /// What it means for a tag to be "appropriate" in this case is that
    ///     1. The registry is the same as that which is referred to in the `REGISTRY` environment variable.
    ///     2. The repository is the same as that which is referred to in the `REPOSITORY` environment variable.
    ///     3. The tag is a valid [RFC 1035 label](names::rfc1035_label).
    ///
    /// If an error occurs, then the temporary image will automatically be destroyed in containerd.
    pub async fn retag(self) -> Result<Push<'a>> {
        let registry = env::registry();
        let repository = env::repository();
        let new_tag = names::rfc1035_label();
        let new_reference = format!("{}/{}:{}", registry, repository, new_tag);
        ctr!(
            "-n",
            &self.image.namespace,
            "images",
            "tag",
            &self.image,
            &new_reference
        )
        .await?;
        Ok(Push {
            // We have a new reference and tag, however the digest
            // and namespace remain unchanged.
            image: TmpImage {
                reference: new_reference,
                tag: new_tag,
                digest: self.image.digest.clone(),
                namespace: self.image.namespace.clone(),
            },
        })
    }
}
