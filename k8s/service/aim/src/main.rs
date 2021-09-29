mod env;
mod registry;

use crate::registry::Image;
use response::Response;
use result::Result;
use rocket::data::{ByteUnit, Limits};
use rocket::fs::TempFile;

#[macro_use]
extern crate rocket;

#[macro_use]
extern crate os;

const MAX_UPLOAD_SIZE: ByteUnit = ByteUnit::Gigabyte(10);

/// Installs the provided OCI compliant image into the this AIM's configured image registry.
/// The maximum size allowed for an image is [10 gigabytes](MAX_UPLOAD_SIZE).
///
/// The metadata for all images installed via this endpoint first undergo a sanitization pipeline
/// before being installed into the registry. That is, the image's original `<repository>:<tag>`
/// information are NOT pushed raw into the registry. The repository is altered to that
/// configured at [env::repository](env::repository). The tag is assigned to a random, UUID backed,
/// [RFC 1035 compliant](names::rfc1035_label) name. For more information on retagging of this
/// image, please see [Retag](registry::containerd::retag::Retag).
///
/// ```text
/// # BASH curl example
/// curl -X POST --data-binary @oracle.img http://aim.ocf-system/install
/// ```
///
/// ```text
/// # Python client exmaple
/// client = Client()
/// image = client.install_from_file("oracle.img")
/// ```
///
/// ```text
/// // Example JSON return structure.
/// {
///   "payload": {
///     "kind": "Image",
///     "object": {
///       "tag": "s0b15278c2f95272de1abc8295775292",
///       "digest": "sha256:7c6243d11b40a87f1f42b56af967889bb312a0343b0350230d182cef210777db"
///     }
///   },
///   "error": null
/// }
/// ```
#[post("/install", data = "<image>")]
async fn install(image: TempFile<'_>) -> Result<Response<Image>> {
    Ok(registry::import(image).await?.into())
}

/// Deletes the given tag from the configured image registry. If the tag is not found, then
/// this endpoint silently succeeds.
///
/// When deleting a tag from ECR, only THAT specific tag is deleted. That is, if the same physical
/// image were installed multiple times, then there would be multiple tags for a single digest.
/// In ECR, the deletion of a tag does NOT affect another tag that is backed by the same digest.
/// Upon the deletion of all tags associated with a digest, then that digest is itself deleted.
/// In a way, digests in ECR are reference counted by the number of tags associated with them. Once
/// the number of tags associated with a digest reaches zero, then the digest is deleted.
///
/// In Minikube, however, the deletion of a tag WILL result in the deletion of the backing digest.
/// Meaning that in development settings, if the same image is installed multiple times, then the
/// deletion of one tag will result in the deletion of all other tags backed by the same digest.
#[delete("/uninstall?<tag>")]
async fn uninstall(tag: String) -> Result<Response<()>> {
    Ok(registry::uninstall(tag).await?.into())
}

/// Returns a list of image objects that is all unique `tag:digest` pairs installed to the registry.
///
/// ```text
/// # BASH curl example
/// curl http://aim.ocf-system/list
/// ```
///
/// ```text
/// # Python client exmaple
/// client = Client()
/// images = client.list()
/// ```
///
/// ```text
/// // Example JSON return structure.
/// {
///   "payload": {
///     "kind": "List[Image]",
///     "object": [
///       {
///         "tag": "n6f7748462d94a093610de86808febbd",
///         "digest": "sha256:cb1ff0854b8864a6a68ee0b5e509d4d94c50a41f96dc2749ea71dc124c89d11f"
///       },
///       {
///         "tag": "p70f18eef60727fb2f9105d78e1e9af2",
///         "digest": "sha256:cb1ff0854b8864a6a68ee0b5e509d4d94c50a41f96dc2749ea71dc124c89d11f"
///       }
///     ]
///   },
///   "error": null
/// }
/// ```
#[get("/list")]
async fn list() -> Result<Response<Vec<Image>>> {
    Ok(registry::list().await?.into())
}

/// Returns a single `tag:digest` object for the given tag. If no such tag exists in the
/// registry, then a [TagNotFound](registry::TagNotFound) error is returned.
///
/// ```text
/// # BASH curl example
/// curl http://aim.ocf-system/get?tag=n6f7748462d94a093610de86808febbd
/// ```
///
/// ```text
/// # Python client exmaple
/// client = Client()
/// image = client.get("n6f7748462d94a093610de86808febbd")
/// ```
///
/// ```text
/// // Example JSON return structure.
/// {
///   "payload": {
///     "kind": "Image",
///     "object": {
///       "tag": "n6f7748462d94a093610de86808febbd",
///       "digest": "sha256:cb1ff0854b8864a6a68ee0b5e509d4d94c50a41f96dc2749ea71dc124c89d11f"
///     }
///   },
///   "error": null
/// }
/// ```
#[get("/get?<tag>")]
async fn get(tag: String) -> Result<Response<Image>> {
    Ok(registry::get(tag).await?.into())
}

#[tokio::main]
async fn main() {
    std::env::set_var("RUST_LOG_STYLE", "always");
    env_logger::init();
    registry::Implementation::configure();
    let config = rocket::Config {
        address: "0.0.0.0".parse().expect("it to parse"),
        limits: Limits::default().limit("file", MAX_UPLOAD_SIZE),
        ..Default::default()
    };
    rocket::custom(config)
        .mount("/", routes![install, uninstall, list, get])
        .launch()
        .await
        .unwrap();
}
