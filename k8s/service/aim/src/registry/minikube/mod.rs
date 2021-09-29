use crate::env;
use crate::registry::Image;
use error::*;
use reqwest::Url;
use result::Result;
use serde::Deserialize;
use sha2::Digest;

/// This module supports the use of the registry plugin with Minikube.
/// It is for development and testing ONLY, so it's behavior is not as well
/// documented nor guaranteed.
///
/// Panics are also allowable in this module. Sunny day scenarios are accounted for,
/// however cases such as the registry not running or an unexpected JSON return structure
/// will panic the thread.

/// An interesting distinction here is that the Minikube registry cannot delete just a single
/// tag - it can only delete digests. So if you give a tag which is backed by a digest that
/// has second tag associated with it (that is, you uploaded the same image twice or more), then
/// they will ALL be deleted from the registry.
///
/// Again, this is Minikube specific behavior.
pub async fn uninstall(tag: String) -> Result<()> {
    let list = list().await?.into_iter().find(|image| image.tag.eq(&tag));
    if list.is_none() {
        return Ok(());
    };
    let digest = list.unwrap().digest;
    let url: Url = format!(
        "http://{}/v2/{}/manifests/{}",
        env::registry(),
        env::repository(),
        digest
    )
    .parse()
    .unwrap();
    let response: reqwest::Response = reqwest::Client::new().delete(url).send().await.unwrap();
    match response.status() {
        reqwest::StatusCode::ACCEPTED | reqwest::StatusCode::NOT_FOUND => Ok(()),
        status => Err(ImageDeleteError { status }.into()),
    }
}

#[derive(Deserialize)]
struct ListTags {
    #[allow(unused)]
    name: String,
    tags: Vec<String>,
}

pub async fn list() -> Result<Vec<Image>> {
    let url: Url = format!(
        "http://{}/v2/{}/tags/list",
        env::registry(),
        env::repository()
    )
    .parse()
    .unwrap();
    let response = reqwest::Client::new().get(url).send().await.unwrap();
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(vec![]);
    }
    let tags: ListTags = response.json().await.unwrap();
    let mut images = vec![];
    let client = reqwest::Client::new();
    for tag in tags.tags {
        let url: Url = format!(
            "http://{}/v2/{}/manifests/{}",
            env::registry(),
            env::repository(),
            tag
        )
        .parse()
        .unwrap();
        let bytes = client
            .get(url)
            .header(
                "Accept",
                "application/vnd.docker.distribution.manifest.v2+json",
            )
            .send()
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap();
        let digest = format!("sha256:{:x}", sha2::Sha256::digest(&bytes));
        images.push(Image { tag, digest })
    }
    Ok(images)
}

pub async fn get<T: AsRef<str>>(tag: T) -> Result<Option<Image>> {
    let url: Url = format!(
        "http://{}/v2/{}/manifests/{}",
        env::registry(),
        env::repository(),
        tag.as_ref()
    )
    .parse()
    .unwrap();
    let response = reqwest::Client::new()
        .get(url)
        .header(
            "Accept",
            "application/vnd.docker.distribution.manifest.v2+json",
        )
        .send()
        .await
        .unwrap();
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let digest = format!(
        "sha256:{:x}",
        sha2::Sha256::digest(&response.bytes().await.unwrap())
    );
    Ok(Some(Image {
        tag: tag.as_ref().to_string(),
        digest,
    }))
}

#[derive(Error, AcmError, Kind, HttpCode, Debug)]
#[code(Status::ServiceUnavailable)]
#[error("Received status code {status} from the registry")]
pub struct ImageDeleteError {
    status: reqwest::StatusCode,
}
