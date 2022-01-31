use crate::prelude::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")] // Sic!
pub struct CreateArtifactRequest {
    r#type:         String,
    name:           String,
    retention_days: Option<u32>,
}

impl CreateArtifactRequest {
    pub fn new(name: impl Into<String>, retention_days: Option<u32>) -> Self {
        CreateArtifactRequest {
            r#type: "actions_storage".to_string(),
            name: name.into(),
            retention_days,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")] // Sic!
pub struct CreateArtifactResponse {
    pub container_id: u64,
    pub size: i64, // must be signed, as -1 is used as a placeholder
    pub signed_content: Option<String>,
    pub file_container_resource_url: Url,
    pub r#type: String,
    pub name: String,
    pub url: Url,
    pub expires_on: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")] // Sic!
pub struct UploadFileQuery {
    pub file:              String,
    pub resource_url:      Url,
    pub max_chunk_size:    i64,
    pub continue_on_error: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")] // Sic!
pub struct PatchArtifactSize {
    pub size: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")] // Sic!
pub struct PatchArtifactSizeResponse {
    pub container_id:   u64,
    pub size:           i64,
    pub signed_content: Option<String>,
    pub r#type:         String,
    pub name:           String,
    pub url:            Url,
    pub upload_url:     Url,
}