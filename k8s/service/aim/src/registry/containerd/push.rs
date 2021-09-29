use crate::ctr;
use crate::env::Secret;
use crate::registry::containerd::tmp_image::TmpImage;
use crate::registry::ecr;
use crate::registry::{Image, Implementation};
use result::Result;

/// The Push step takes ownership of a [TmpImage](TmpImage) and offers
/// a single method...[Push::push](Push::push).
pub struct Push<'a> {
    pub image: TmpImage<'a>,
}

impl<'a> Push<'a> {
    pub async fn push(self) -> Result<Image> {
        match Implementation::which() {
            Implementation::ECR => self.push_to_ecr().await?,
            Implementation::Minikube => self.push_to_minikube().await?,
        };
        Ok(self.image.into())
    }

    async fn push_to_ecr(&self) -> Result<()> {
        let (username, password) = ecr::get_credentials().await?;
        let credentials = Secret::from(format!("{}:{}", username, password.raw_secret()));
        ctr!(
            "-n",
            &self.image.namespace,
            "images",
            "push",
            "-u",
            &credentials,
            &self.image
        )
        .await
        .map(|_| ())
    }

    async fn push_to_minikube(&self) -> Result<()> {
        ctr!(
            "-n",
            &self.image.namespace,
            "images",
            "push",
            "--plain-http",
            &self.image
        )
        .await
        .map(|_| ())
    }
}
