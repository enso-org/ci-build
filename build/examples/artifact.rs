use reqwest::header::HeaderMap;
use ide_ci::prelude::*;

#[tokio::main]
async fn main() -> Result {
    // see https://github.com/check-spelling/check-spelling/wiki/%40actions-upload-artifact
    let url = ide_ci::env::expect_var("ACTIONS_RUNTIME_URL")?;
    let token = ide_ci::env::expect_var("ACTIONS_RUNTIME_TOKEN")?;
    let run_id = ide_ci::env::expect_var("GITHUB_RUN_ID")?;
    let api_version = "6.0-preview";

    let artifact_url =  iformat!("{url}_apis/pipelines/workflows/{run_id}/artifacts?api-version={api_version}");

    let mut headers = HeaderMap::new();
    headers.insert(reqwest::header::ACCEPT, iformat!("application/json;api-version={api_version}").into());
    headers.insert(reqwest::header::CONTENT_TYPE, "application/json".into());
    //headers.insert()

    Ok(())
    //let client = reqwest::Client::builder().default_headers().
}
