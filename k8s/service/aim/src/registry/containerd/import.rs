use super::namespace::Namespace;
use crate::ctr;
use crate::registry::containerd::retag::Retag;
use crate::registry::containerd::tmp_image::TmpImage;
use error::*;
use kind::Kind;
use result::Result;
use rocket::fs::TempFile;
use std::path::Path;
use thiserror::Error;

/// The `Import` step takes in the current working [Namespace](Namespace) and imports the given
/// image, as is, into that namespace and extracts that images original metadata.
///
/// No further transformations on that image are done at this time.
pub struct Import<'a> {
    pub namespace: &'a Namespace,
}

/// A reference is the fully qualified image reference. For example, `docker.io/alation/ocf/aim:1.0.0`
type Reference = String;
type Digest = String;
type Tag = String;

impl<'a> Import<'a> {
    /// Imports the given temporary file into containerd and returns a [Retaggin](Retag) step.
    ///
    /// This function takes ownership of the provided temporary file and deletes it upon completion.
    pub async fn import(self, tmp: TempFile<'_>) -> Result<Retag<'a>> {
        self.import_path(tmp.path().unwrap().to_str().unwrap().to_string())
            .await
    }

    /// Imports the given file path into containerd and returns a [Retaggin](Retag) step.
    pub async fn import_path<P: AsRef<Path>>(self, path: P) -> Result<Retag<'a>> {
        let path = path.as_ref().to_str().ok_or_else(|| TempPathIsNotUFT8 {
            path: format!("{}", path.as_ref().as_os_str().to_string_lossy()),
        })?;
        // Possibly figure out what file type it actually is
        // https://crates.io/crates/infer
        ctr!(
            "-n",
            &self.namespace,
            "images",
            "import",
            "--no-unpack",
            &path
        )
        .await?;
        Ok(Retag {
            image: Self::extract_image_metadata(self.namespace).await?,
        })
    }

    /// Runs `ctr -n <NAMESPACE> images ls` and attempts to extract the reference, tag, and digest
    /// of the image that we just installed to that namespace.
    ///
    /// We are expecting output similar to the following...
    ///
    /// ```text
    /// REF                          TYPE                                                 DIGEST                                                                  SIZE     PLATFORMS   LABELS
    /// docker.io/test/tennis:latest application/vnd.docker.distribution.manifest.v2+json sha256:76a5627069e32d0543dd6bec4c352af358974dd4572dfc05dbf7147b5546df4f 19.2 MiB linux/amd64 -      
    /// ```
    async fn extract_image_metadata(namespace: &Namespace) -> Result<TmpImage<'_>> {
        let images_ls = ctr!("-n", namespace, "images", "ls").await?;
        let (reference, tag, digest) = Self::extract_image_metadata_from_str(namespace, images_ls)?;
        let image = TmpImage {
            reference,
            tag,
            digest,
            namespace,
        };
        Ok(image)
    }

    /// Takes in the result of running `ctr -n <NAMESPACE> images ls` and attempts
    /// to extract the reference, tag, and digest of the image that we just installed to
    /// that namespace.
    ///
    /// We are expecting output similar to the following...
    ///
    /// ```text
    /// REF                          TYPE                                                 DIGEST                                                                  SIZE     PLATFORMS   LABELS
    /// docker.io/test/tennis:latest application/vnd.docker.distribution.manifest.v2+json sha256:76a5627069e32d0543dd6bec4c352af358974dd4572dfc05dbf7147b5546df4f 19.2 MiB linux/amd64 -      
    /// ```
    fn extract_image_metadata_from_str<T: AsRef<str>, U: AsRef<str>>(
        namespace: T,
        images_ls: U,
    ) -> Result<(Reference, Tag, Digest)> {
        let images_ls_vec = images_ls.as_ref().split('\n').collect::<Vec<&str>>();
        let image: &str = match images_ls_vec[..] {
            [_, data] => data,
            [header] => {
                return Err(CtrImageLs::NoData {
                    namespace: namespace.as_ref().to_string(),
                    header: header.to_string(),
                }
                .into())
            }
            _ => {
                return Err(UnexpectedContainerdImageLsFormat {
                    output: images_ls.as_ref().to_string(),
                }
                .into())
            }
        };
        let (reference, digest) = match image.split(' ').collect::<Vec<&str>>()[..] {
            [reference, _, digest, ..] => (reference.to_string(), digest.to_string()),
            [..] => {
                return Err(UnexpectedContainerdImageRow {
                    output: images_ls.as_ref().to_string(),
                }
                .into())
            }
        };
        let tag = match reference.rsplit_once(":") {
            Some((_, tag)) => tag.to_string(),
            None => {
                return Err(UnexpectedImageReferenceFormat {
                    output: reference.clone(),
                }
                .into())
            }
        };
        Ok(((reference as Reference), (tag as Tag), (digest as Digest)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const TEST_DATA: &str= "REF                          TYPE                                                 DIGEST                                                                  SIZE     PLATFORMS   LABELS \ndocker.io/test/tennis:latest application/vnd.docker.distribution.manifest.v2+json sha256:76a5627069e32d0543dd6bec4c352af358974dd4572dfc05dbf7147b5546df4f 19.2 MiB linux/amd64 -      ";

    #[test]
    fn test_extract_image_metadata() {
        let (reference, _, digest) =
            Import::extract_image_metadata_from_str("some namespace", TEST_DATA).unwrap();
        assert_eq!(reference, "docker.io/test/tennis:latest");
        assert_eq!(
            digest,
            "sha256:76a5627069e32d0543dd6bec4c352af358974dd4572dfc05dbf7147b5546df4f"
        )
    }
}

#[derive(Error, Kind, AcmError, HttpCode, Debug)]
#[error(
    "We received unexpected output from containerd at the \"import\" phase of our workflow. \
We expected a header of \"REF TYPE DIGEST SIZE PLATFORMS LABELS\" followed by a single row, \
but instead we got the following raw output: \"{output}\""
)]
#[code(Status::InternalServerError)]
struct UnexpectedContainerdImageLsFormat {
    output: String,
}

#[derive(Error, Kind, AcmError, HttpCode, Debug)]
#[error(
    "We received unexpected output from containerd at the \"import\" phase of our workflow. \
We expected a header of \"REF TYPE DIGEST SIZE PLATFORMS LABELS\" followed by a single row, \
but instead we got the following raw output: \"{output}\""
)]
#[code(Status::InternalServerError)]
struct UnexpectedContainerdImageRow {
    output: String,
}

#[derive(Error, Kind, AcmError, HttpCode, Debug)]
#[error(
    "We received unexpected output from containerd at the \"import\" phase of our workflow. \
We expected a header of \"REF TYPE DIGEST SIZE PLATFORMS LABELS\" followed by a single row whose \
\"REF\" section is of the format \"<registry>/<repository>:<tag>\"
bit instead we got the following raw output: \"{output}\""
)]
#[code(Status::InternalServerError)]
struct UnexpectedImageReferenceFormat {
    output: String,
}

#[derive(Error, Kind, AcmError, HttpCode, Debug)]
#[error("The temporary filepath for the connector's image was not valid UTF8 (was (lossy) {path}). This is concerning, although the install may work if you just try again.")]
#[code(Status::InternalServerError)]
struct TempPathIsNotUFT8 {
    path: String,
}

#[derive(Error, Kind, AcmError, HttpCode, Debug)]
pub enum CtrImageLs {
    #[error("Failed to list images for namespace {namespace}. We appear to have only received the table header (got {header})")]
    #[code(Status::InternalServerError)]
    NoData { namespace: String, header: String },
}
