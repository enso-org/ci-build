use crate::prelude::*;

use crate::actions::artifacts::context::Context;
use crate::actions::artifacts::models::ArtifactResponse;
use crate::actions::artifacts::models::CreateArtifactResponse;
use crate::actions::artifacts::models::PatchArtifactSizeResponse;
use crate::actions::artifacts::raw;

use reqwest::Client;

#[derive(Clone, Debug)]
pub struct SessionClient {
    pub json_client:   Client,
    pub binary_client: Client,
    pub artifact_url:  Url,
}

impl SessionClient {
    pub async fn create_container(
        &self,
        artifact_name: impl AsRef<str>,
    ) -> Result<CreateArtifactResponse> {
        raw::endpoints::create_container(
            &self.json_client,
            self.artifact_url.clone(),
            artifact_name,
        )
        .await
    }

    pub async fn list_artifacts(&self) -> Result<Vec<ArtifactResponse>> {
        raw::endpoints::list_artifacts(&self.json_client, self.artifact_url.clone()).await
    }

    pub fn new(context: &Context) -> Result<Self> {
        Ok(Self {
            json_client:   context.json_client()?,
            binary_client: context.binary_client()?,
            artifact_url:  context.artifact_url()?,
        })
    }
    pub async fn patch_artifact_size(
        &self,
        artifact_name: &str,
        total_size: usize,
    ) -> Result<PatchArtifactSizeResponse> {
        raw::endpoints::patch_artifact_size(
            &self.json_client,
            self.artifact_url.clone(),
            artifact_name,
            total_size,
        )
        .await
    }
}
