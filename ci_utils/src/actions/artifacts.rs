use crate::prelude::*;
use std::collections::VecDeque;
use std::fmt::Formatter;
use std::fs::Metadata;
use std::ops::DerefMut;
use std::ops::Range;
use std::ops::RangeInclusive;
use std::sync::Mutex;

use crate::actions::artifacts::models::CreateArtifactRequest;
use crate::actions::artifacts::models::CreateArtifactResponse;
use crate::actions::artifacts::models::PatchArtifactSize;
use crate::actions::artifacts::models::PatchArtifactSizeResponse;
use crate::env::expect_var;
use chrono::Duration;
use futures_util::future::err;
use reqwest::header::HeaderMap;
use reqwest::header::HeaderValue;
use reqwest::Body;
use reqwest::Client;
use reqwest::ClientBuilder;
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use tokio::io::AsyncReadExt;

pub mod models;

pub const API_VERSION: &str = "6.0-preview";

#[derive(Clone, Debug)]
pub struct Context {
    pub runtime_url:   Url,
    pub runtime_token: String,
    pub run_id:        String,
    pub api_version:   String,
}

impl Context {
    pub fn new() -> Result<Self> {
        let runtime_url = expect_var("ACTIONS_RUNTIME_URL")?.parse()?;
        let runtime_token = expect_var("ACTIONS_RUNTIME_TOKEN")?;
        let run_id = expect_var("GITHUB_RUN_ID")?;
        let api_version = API_VERSION.to_string();
        Ok(Context { runtime_url, runtime_token, run_id, api_version })
    }

    pub fn artifact_url(&self) -> Result<Url> {
        let Context { runtime_url, run_id, api_version, .. } = self;
        let url_text = iformat!(
            "{runtime_url}_apis/pipelines/workflows/{run_id}/artifacts?api-version={api_version}"
        );
        Url::parse(&url_text).anyhow_err()
    }

    pub fn prepare_client(&self, f: impl FnOnce(ClientBuilder) -> ClientBuilder) -> Result<Client> {
        let mut headers = HeaderMap::new();
        headers.insert(
            reqwest::header::ACCEPT,
            iformat!("application/json;api-version={self.api_version}").parse()?,
        );
        headers.insert(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", self.runtime_token).parse()?,
        );
        //
        // if let Some(keep_alive) = keep_alive {
        //     headers.insert(reqwest::header::CONNECTION, "Keep-Alive".parse()?);
        //     headers.insert("Keep-Alive", keep_alive.num_seconds().into());
        // }
        //
        //
        let base_builder =
            ClientBuilder::new().default_headers(headers).user_agent(crate::USER_AGENT);
        f(base_builder).build().anyhow_err()
    }
}

#[derive(Clone, Debug)]
pub struct FileToUpload {
    /// Absolute path in the local filesystem.
    local_path:  PathBuf,
    /// Relative path within the artifact container. Does not include the leading segment with the
    /// artifact name.
    remote_path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct UploadOptions {
    pub file_concurrency:  usize,
    pub chunk_size:        usize,
    // by default, file uploads will continue if there is an error unless specified differently in
    // the options
    pub continue_on_error: bool,
}

#[derive(Clone, Debug)]
pub struct UploadResult {
    pub is_success:             bool,
    pub successful_upload_size: usize,
    pub total_size:             usize,
}

#[derive(Debug, Default)]
struct State {
    pub to_upload:        VecDeque<FileToUpload>,
    pub upload_file_size: usize,
    pub total_file_size:  usize,
    pub abort:            bool,
}

impl State {
    pub fn next_job(&mut self) -> Option<FileToUpload> {
        if self.abort {
            None
        } else {
            self.to_upload.pop_front()
        }
    }

    pub fn process_result(&mut self, continue_on_error: bool, result: UploadResult) {
        self.upload_file_size += result.successful_upload_size;
        self.total_file_size += result.total_size;
        if !result.is_success && !continue_on_error {
            self.abort = true
        }
    }

    pub fn add_tasks(&mut self, tasks: impl IntoIterator<Item = FileToUpload>) {
        self.to_upload.extend(tasks)
    }
}

#[derive(Clone, Debug, Default)]
pub struct StateHandle(Arc<Mutex<State>>);

impl StateHandle {
    fn with<R>(&self, f: impl FnOnce(&mut State) -> R) -> R {
        let mut guard = self.0.lock().unwrap();
        f(guard.deref_mut())
    }

    pub fn next_job(&self) -> Option<FileToUpload> {
        self.with(|s| s.next_job())
    }

    pub fn process_result(&self, continue_on_error: bool, result: UploadResult) {
        self.with(|s| s.process_result(continue_on_error, result))
    }

    pub fn add_tasks(&self, tasks: impl IntoIterator<Item = FileToUpload>) {
        self.with(|s| s.add_tasks(tasks))
    }

    pub fn get_total_size(&self) -> usize {
        self.with(|s| s.total_file_size)
    }
}

pub struct ContentRange {
    pub range: RangeInclusive<usize>,
    pub total: Option<usize>,
}

impl ContentRange {
    pub fn whole(len: usize) -> Self {
        Self { range: 0..=len.saturating_sub(1), total: Some(len) }
    }
}

impl Display for ContentRange {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} - {}/{}",
            self.range.start(),
            self.range.end(),
            self.total.map_or(String::from("*"), |total| total.to_string())
        )
    }
}

pub struct UploadWorker {}

#[derive(Clone, Debug)]
pub struct ArtifactHandler {
    pub json_client:      Client,
    pub binary_client:    Client,
    pub context:          Context,
    ongoing_upload_state: StateHandle,
}

impl ArtifactHandler {
    pub fn new(context: &Context) -> Result<Self> {
        let keep_alive_seconds = 3;
        let json_client = context.prepare_client(|builder| {
            let mut headers = HeaderMap::new();
            headers.insert(
                reqwest::header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            );
            builder.default_headers(headers)
        })?;

        let binary_client = context.prepare_client(|builder| {
            let mut headers = HeaderMap::new();
            headers.insert(
                reqwest::header::CONTENT_TYPE,
                HeaderValue::from_static("application/octet-stream"),
            );
            headers.insert(reqwest::header::CONNECTION, HeaderValue::from_static("Keep-Alive"));
            headers.insert("Keep-Alive", keep_alive_seconds.into());
            builder.default_headers(headers)
        })?;
        Ok(ArtifactHandler {
            json_client,
            binary_client,
            context: context.clone(),
            ongoing_upload_state: default(),
        })
    }


    /// Creates a file container for the new artifact in the remote blob storage/file service.
    ///
    /// Returns the response from the Artifact Service if the file container was successfully
    /// create.
    #[context("Failed to create a file container for the new  artifact `{}`.", artifact_name.as_ref())]
    pub async fn create_container(
        &self,
        artifact_name: impl AsRef<str>,
    ) -> Result<CreateArtifactResponse> {
        let body = CreateArtifactRequest::new(artifact_name.as_ref(), None);
        let url = self.context.artifact_url()?;
        let request = self
            .json_client
            .post(url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .json(&body);

        dbg!(&request);
        // TODO retry
        let response = request.send().await?;
        dbg!(&response);
        let status = response.status();
        if status.is_success() {
            let err_body = response.text().await?;
            let err = anyhow!("Server replied with {}. Response body: {}", status, err_body);

            let err = match status {
                StatusCode::FORBIDDEN => err.context(
                    "Artifact storage quota has been hit. Unable to upload any new artifacts.",
                ),
                StatusCode::BAD_REQUEST => err
                    .context(format!("The artifact name {} is not valid.", artifact_name.as_ref())),
                _ => err,
            };
            Err(err)
        } else {
            response.json().await.anyhow_err()
        }
    }

    /// Concurrently upload all of the files in chunks.
    pub async fn upload_artifact_to_file_container(
        &self,
        upload_url: &Url,
        files_to_upload: Vec<FileToUpload>,
        options: &UploadOptions,
    ) -> Result {
        println!(
            "File Concurrency: {}, and Chunk Size: {}.",
            options.file_concurrency, options.chunk_size
        );

        println!("Will upload {} files.", files_to_upload.len());
        self.ongoing_upload_state.add_tasks(files_to_upload);

        let mut tasks = Vec::new();
        for task_index in 0..options.file_concurrency {
            let continue_on_error = options.continue_on_error;
            let uploader =
                FileUploader { url: upload_url.clone(), client: self.binary_client.clone() };
            let state = self.ongoing_upload_state.clone();
            let task = async move {
                while let Some(file_to_upload) = state.next_job() {
                    println!("Will upload file {}.", file_to_upload.local_path.display());
                    let result = match uploader.upload_file(&file_to_upload).await {
                        Ok(len) => UploadResult {
                            is_success:             true,
                            total_size:             len,
                            successful_upload_size: len,
                        },
                        Err(e) => {
                            println!(
                                "Failed to upload {}: {}",
                                file_to_upload.local_path.display(),
                                e
                            );
                            UploadResult {
                                is_success:             false,
                                total_size:             0,
                                successful_upload_size: 0,
                            }
                        }
                    };

                    println!(
                        "Finished uploading file {}. Result: {:?}",
                        file_to_upload.local_path.display(),
                        result
                    );
                    state.process_result(continue_on_error, result);
                }
                Result::Ok(())
            };
            tasks.push(task);
        }

        let mut task_handles = tasks.into_iter().map(tokio::task::spawn).collect_vec();
        let _rets = futures_util::future::join_all(task_handles).await;

        Ok(())
    }

    // pub fn upload_file(
    //     &self,
    //     upload_url: &Url,
    //     file_to_upload: &FileToUpload,
    // ) -> BoxFuture<UploadResult> {
    //     async move {}.boxed()
    // }


    #[context("Failed to finalize upload of the artifact `{}`.", artifact_name)]
    pub async fn patch_artifact_size(
        &self,
        artifact_name: &str,
    ) -> Result<PatchArtifactSizeResponse> {
        let artifact_url = self.context.artifact_url()?;

        let patch_request = self
            .json_client
            .patch(artifact_url.clone())
            .query(&[("artifactName", artifact_name)]) // OsStr can be passed here, fails runtime
            .json(&PatchArtifactSize { size: self.ongoing_upload_state.get_total_size() });

        // TODO retry
        let response = patch_request.send().await?;
        Ok(response.json().await?)
    }
}

pub struct FileUploader {
    pub url:    Url,
    pub client: Client,
}

impl FileUploader {
    pub async fn upload_file(&self, file_to_upload: &FileToUpload) -> Result<usize> {
        let file = tokio::fs::File::open(&file_to_upload.local_path).await?;
        let len = file.metadata().await?.len() as usize;
        let body = Body::from(file);
        let response = self
            .client
            .put(self.url.clone())
            .query(&[("itemPath", &file_to_upload.remote_path)])
            .header(reqwest::header::CONTENT_LENGTH, len)
            .header(reqwest::header::CONTENT_RANGE, ContentRange::whole(len as usize).to_string())
            .body(body)
            .send()
            .await?;
        dbg!(&response);
        response.error_for_status()?;
        Ok(len)
    }
}

pub async fn execute_dbg<T: DeserializeOwned + std::fmt::Debug>(
    client: &reqwest::Client,
    reqeust: reqwest::RequestBuilder,
) -> Result<T> {
    let request = reqeust.build()?;
    dbg!(&request);
    let response = client.execute(request).await?;
    dbg!(&response);
    let text = response.text().await?;
    println!("{}", &text);
    let deserialized = serde_json::from_str(&text)?;
    dbg!(&deserialized);
    Ok(deserialized)
}

pub async fn upload_path(path: impl AsRef<Path>) -> Result {
    let filename = path.as_ref().file_name().unwrap();
    let name = filename.to_str().unwrap();

    let options = UploadOptions {
        chunk_size:        8_000_000,
        file_concurrency:  10,
        continue_on_error: true,
    };

    let files = vec![FileToUpload {
        local_path:  path.as_ref().to_path_buf(),
        remote_path: PathBuf::from(filename).join(name),
    }];

    let context = Context::new()?;
    let mut handler = ArtifactHandler::new(&context)?;
    let container = handler.create_container(name).await?;
    handler.upload_artifact_to_file_container(&container.url, files, &options).await?;
    handler.patch_artifact_size(name).await?;
    Ok(())
}

//
// pub async fn upload_file(path: impl AsRef<Path>, artifact_name: &str) -> Result {
//     // see https://github.com/check-spelling/check-spelling/wiki/%40actions-upload-artifact
//     let context = Context::new()?;
//
//     let client = context.prepare_client(|builder| keep_alive(builder, 10))?;
//
//     let artifact_url = context.artifact_url()?;
//     let query = CreateArtifactRequest::new(artifact_name, Some(3));
//     let create_request = client
//         .post(artifact_url.clone())
//         .header(reqwest::header::CONTENT_TYPE, "application/json")
//         .json(&query);
//     let created_artifact: CreateArtifactResponse = execute_dbg(&client, create_request).await?;
//
//     // Upload file to container.
//     let upload_url = created_artifact.file_container_resource_url.clone();
//
//
//     let artifact_path = path.as_ref().file_name().unwrap(); // FIXME
//     let file = Bytes::from(std::fs::read(&path)?);
//     let upload_request = client
//         .put(upload_url)
//         .query(&[("itemPath",
// PathBuf::from(artifact_name).join(artifact_path.to_str().unwrap()))])         .header(reqwest::
// header::CONTENT_TYPE, "application/octet-stream")         .header(reqwest::header::
// CONTENT_LENGTH, file.len())         .header(reqwest::header::CONTENT_RANGE, iformat!("bytes
// 0-{file.len() - 1}/{file.len()}"))         .body(file.clone());
//     let upload_response: serde_json::Value = execute_dbg(&client, upload_request).await?;
//
//     let patch_request = client
//         .patch(artifact_url.clone())
//         .query(&[("artifactName", artifact_name)]) // OsStr can be passed here, fails runtime
//         .header(reqwest::header::CONTENT_TYPE, "application/json")
//         .json(&PatchArtifactSize { size: file.len() });
//
//     let patch_response: serde_json::Value = execute_dbg(&client, patch_request).await?;
//
//     //
//     //     // see https://github.com/check-spelling/check-spelling/wiki/%40actions-upload-artifact
//     //     let url = ide_ci::env::expect_var("ACTIONS_RUNTIME_URL")?;
//     //     let token = ide_ci::env::expect_var("ACTIONS_RUNTIME_TOKEN")?;
//     //     let run_id = ide_ci::env::expect_var("GITHUB_RUN_ID")?;
//     //     let api_version = "6.0-preview";
//     //
//     //     let artifact_url =
//     // iformat!("{url}_apis/pipelines/workflows/{run_id}/artifacts?api-version={api_version}");
//     //
//     //     let mut headers = HeaderMap::new();
//     //     headers.insert(reqwest::header::ACCEPT,
//     // iformat!("application/json;api-version={api_version}").into());     headers.
//     // insert(reqwest::header::CONTENT_TYPE, "application/json".into());     headers.
//     // insert(reqwest::header::AUTHORIZATION, format!("Bearer {}", pat).parse()?);
//     //
//     //     //headers.insert()
//     //
//     Ok(())
// }

// #[tokio::main]
// async fn main() -> Result {
//     upload_file("Cargo.toml", "SomeFile").await?;
//     Ok(())
//     //let client = reqwest::Client::builder().default_headers().
// }


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_response() -> Result {
        let text = r#"{"containerId":11099678,"size":-1,"signedContent":null,"fileContainerResourceUrl":"https://pipelines.actions.githubusercontent.com/VYS7uSE1JB12MkavBOHvD6nounefzg1s5vHmQvfbiLmuvFuM6c/_apis/resources/Containers/11099678","type":"actions_storage","name":"SomeFile","url":"https://pipelines.actions.githubusercontent.com/VYS7uSE1JB12MkavBOHvD6nounefzg1s5vHmQvfbiLmuvFuM6c/_apis/pipelines/1/runs/75/artifacts?artifactName=SomeFile","expiresOn":"2022-01-29T04:07:24.5807079Z","items":null}"#;
        let response = serde_json::from_str::<CreateArtifactResponse>(text)?;
        //
        // let patch_request = client.patch(artifact_url.clone())
        //     .query(&[("artifactName", artifact_name)])
        //     .header(reqwest::header::CONTENT_TYPE, "application/json")
        //     .json(&PatchArtifactSize {size: file.len()});

        let path = PathBuf::from("Cargo.toml");
        let artifact_path = path.file_name().unwrap(); // FIXME

        let client = reqwest::ClientBuilder::new().build()?;
        dbg!(artifact_path);
        client
            .patch(response.url)
            .query(&[("itemPath", artifact_path.to_str().unwrap())])
            .build()?;

        Ok(())
    }
}
