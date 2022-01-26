use chrono::Duration;
use reqwest::{Client, ClientBuilder};
use reqwest::header::HeaderMap;
use serde::de::DeserializeOwned;
use ide_ci::github::create_client;
use ide_ci::prelude::*;

pub struct Context {
    pub runtime_url: Url,
    pub runtime_token: String,
    pub run_id: String,
    pub api_version: String,
}

impl Context {
    pub fn new() -> Result<Self> {
        let runtime_url = ide_ci::env::expect_var("ACTIONS_RUNTIME_URL")?.parse()?;
        let runtime_token = ide_ci::env::expect_var("ACTIONS_RUNTIME_TOKEN")?;
        let run_id = ide_ci::env::expect_var("GITHUB_RUN_ID")?;
        let api_version = "6.0-preview".to_string();
        Ok(Context { runtime_url, runtime_token, run_id, api_version })
    }

    pub fn artifact_url(&self) -> Result<Url> {
        let Context{runtime_url, run_id, api_version, ..} = self;
        let url_text = iformat!("{runtime_url}_apis/pipelines/workflows/{run_id}/artifacts?api-version={api_version}");
        Url::parse(&url_text).anyhow_err()
    }

    pub fn prepare_json_client(&self, keep_alive: Option<Duration>) -> Result<Client> {
        let mut headers = HeaderMap::new();
        headers.insert(reqwest::header::ACCEPT, iformat!("application/json;api-version={self.api_version}").parse()?);
        // headers.insert(reqwest::header::ACCEPT_ENCODING, "gzip".into());
        // headers.insert(reqwest::header::ACCEPT, iformat!("application/octet-stream;api-version={api_version}").into());
        headers.insert(reqwest::header::CONTENT_TYPE, "application/json".parse()?);
        headers.insert(reqwest::header::AUTHORIZATION, format!("Bearer {}", self.runtime_token).parse()?);

        if let Some(keep_alive) = keep_alive {
            headers.insert(reqwest::header::CONNECTION, "Keep-Alive".parse()?);
            headers.insert("Keep-Alive", keep_alive.num_seconds().into());
        }

        ClientBuilder::new().default_headers(headers).user_agent(ide_ci::USER_AGENT).build().anyhow_err()
    }

    // pub fn prepare_bin_client(&self, keep_alive: Option<Duration>) -> Result<Client> {
    //     let mut headers = HeaderMap::new();
    //     headers.insert(reqwest::header::ACCEPT, iformat!("application/json;api-version={self.api_version}").parse()?);
    //     // headers.insert(reqwest::header::ACCEPT_ENCODING, "gzip".into());
    //     // headers.insert(reqwest::header::ACCEPT, iformat!("application/octet-stream;api-version={api_version}").into());
    //     headers.insert(reqwest::header::CONTENT_TYPE, "application/json".parse()?);
    //     headers.insert(reqwest::header::AUTHORIZATION, format!("Bearer {}", self.runtime_token).parse()?);
    //
    //     if let Some(keep_alive) = keep_alive {
    //         headers.insert(reqwest::header::CONNECTION, "Keep-Alive".parse()?);
    //         headers.insert("Keep-Alive", keep_alive.num_seconds().into());
    //     }
    //
    //     ClientBuilder::new().default_headers(headers).user_agent(ide_ci::USER_AGENT).build().anyhow_err()
    // }
}

#[derive(Clone,Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")] // Sic!
pub struct CreateArtifactRequest {
    r#type: String,
    name: String,
    retention_days: Option<u32>,
}

impl CreateArtifactRequest {
    pub fn new(name: impl Into<String>) -> Self {
        CreateArtifactRequest {
            r#type: "actions_storage".to_string(),
            name: name.into(),
            retention_days: Some(3),
        }
    }
}

#[derive(Clone,Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")] // Sic!
pub struct CreateArtifactResponse {
    pub container_id: u64,
    pub size: i64,
    pub signed_content: String,
    pub file_container_resource_url: String,
    pub r#type: String,
    pub name: String,
    pub url: String,
}

#[derive(Clone,Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")] // Sic!
pub struct UploadFileQuery {
    pub file: String,
    pub resource_url: Url,
    pub max_chunk_size: i64,
    pub continue_on_error: bool,
}

// #[derive(Clone,Debug, Serialize, Deserialize)]
// #[serde(rename_all = "camelCase")] // Sic!
// pub struct UploadFileResponse {
//     pub is_success: bool,
//     pub successful_upload_size: i64,
//     pub total_size: i64,
// }

#[derive(Clone,Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")] // Sic!
pub struct PatchArtifactSize {
    pub size: usize,
}

#[derive(Clone,Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")] // Sic!
pub struct PatchArtifactSizeResponse {
    pub container_id: u64,
    pub size: i64,
    pub signed_content: String,
    pub r#type: String,
    pub name: String,
    pub url: Url,
    upload_url: Url,
}

pub async fn execute_dbg<T: DeserializeOwned + std::fmt::Debug>(client: &reqwest::Client, reqeust: reqwest::RequestBuilder) -> Result<T> {
    let request = reqeust.build()?;
    dbg!(&request);
    let response = client.execute(request).await?;
    dbg!(&response);
    let text = response.text().await?;
    dbg!(&text);
    let deserialized = serde_json::from_str(&text)?;
    dbg!(&deserialized);
    Ok(deserialized)
}

pub async fn upload_file(path: impl AsRef<Path>, artifact_name: &str) -> Result {
    // see https://github.com/check-spelling/check-spelling/wiki/%40actions-upload-artifact
    let context = Context::new()?;

    let client = context.prepare_json_client(Some(Duration::seconds(10)))?;
    // let mut headers = HeaderMap::new();
    // headers.insert(reqwest::header::ACCEPT, iformat!("application/json;api-version={api_version}").into());
    // headers.insert(reqwest::header::CONTENT_TYPE, "application/json".into());
    // headers.insert(reqwest::header::AUTHORIZATION, format!("Bearer {}", pat).parse()?);

    let artifact_url  =context.artifact_url()?;


    let query = CreateArtifactRequest::new(artifact_name);
    let created_artifact: CreateArtifactResponse = execute_dbg(&client, client.post(artifact_url.clone()).json(&query)).await?;

    // Upload file to container.
    let upload_url = created_artifact.url.clone();


    let artifact_path = path.as_ref().file_name().unwrap(); // FIXME
    let file = std::fs::read_to_string(&path)?;

    let upload_request = client.put(upload_url)
        .query(&[("itemPath", artifact_path)])
        .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
        .header(reqwest::header::CONTENT_LENGTH, file.len());

    let upload_response:serde_json::Value = execute_dbg(&client, upload_request).await?;

    let patch_request = client.patch(artifact_url.clone()).query(&[("artifactName", artifact_name)]).json(&PatchArtifactSize {size: file.len()});

    let patch_response:serde_json::Value = execute_dbg(&client, patch_request).await?;

//
//     // see https://github.com/check-spelling/check-spelling/wiki/%40actions-upload-artifact
//     let url = ide_ci::env::expect_var("ACTIONS_RUNTIME_URL")?;
//     let token = ide_ci::env::expect_var("ACTIONS_RUNTIME_TOKEN")?;
//     let run_id = ide_ci::env::expect_var("GITHUB_RUN_ID")?;
//     let api_version = "6.0-preview";
//
//     let artifact_url =  iformat!("{url}_apis/pipelines/workflows/{run_id}/artifacts?api-version={api_version}");
//
//     let mut headers = HeaderMap::new();
//     headers.insert(reqwest::header::ACCEPT, iformat!("application/json;api-version={api_version}").into());
//     headers.insert(reqwest::header::CONTENT_TYPE, "application/json".into());
//     headers.insert(reqwest::header::AUTHORIZATION, format!("Bearer {}", pat).parse()?);
//
//     //headers.insert()
//
    Ok(())
}

#[tokio::main]
async fn main() -> Result {
    upload_file("Cargo.toml", "SomeFile").await?;
    Ok(())
    //let client = reqwest::Client::builder().default_headers().
}
