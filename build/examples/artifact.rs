use chrono::Duration;
use reqwest::{Client, ClientBuilder};
use reqwest::header::HeaderMap;
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

    pub fn prepare_client(&self, keep_alive: Option<Duration>) -> Result<Client> {
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
            retention_days: None,
        }
    }
}

#[derive(Clone,Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")] // Sic!
pub struct CreateArtifactResponse {
    pub container_id: u64,
    pub size: u64,
    pub signed_content: String,
    pub file_container_resource_url: String,
    pub r#type: String,
    pub name: String,
    pub url: String,
}

pub async fn upload_file(path: impl AsRef<Path>, artifact_name: &str) -> Result {
    // see https://github.com/check-spelling/check-spelling/wiki/%40actions-upload-artifact
    let context = Context::new()?;

    let client = context.prepare_client(Some(Duration::seconds(10)))?;
    let mut headers = HeaderMap::new();
    // headers.insert(reqwest::header::ACCEPT, iformat!("application/json;api-version={api_version}").into());
    // headers.insert(reqwest::header::CONTENT_TYPE, "application/json".into());
    // headers.insert(reqwest::header::AUTHORIZATION, format!("Bearer {}", pat).parse()?);

    let query = CreateArtifactRequest::new(artifact_name);

    let request = client.post(&context.artifact_url()).body(query).build()?;
    dbg!(&request);
    let response = client.execute(request).await?;
    dbg!(&response);
    let response = response.json::<CreateArtifactResponse>().await?;
    dbg!(&response);


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
    let context = Context::new()?;
    // see https://github.com/check-spelling/check-spelling/wiki/%40actions-upload-artifactinsert()

    Ok(())
    //let client = reqwest::Client::builder().default_headers().
}
