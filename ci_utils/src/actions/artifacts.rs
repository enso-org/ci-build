use crate::actions::artifacts::models::CreateArtifactRequest;
use crate::actions::artifacts::models::CreateArtifactResponse;
use crate::actions::artifacts::models::PatchArtifactSize;
use crate::actions::artifacts::models::PatchArtifactSizeResponse;
use crate::env::expect_var;
use crate::prelude::*;
use anyhow::Context as Trait_anyhow_Context;
use bytes::BytesMut;
use flume::Sender;
use reqwest::header::HeaderMap;
use reqwest::header::HeaderValue;
use reqwest::header::InvalidHeaderValue;
use reqwest::Body;
use reqwest::Client;
use reqwest::ClientBuilder;
use reqwest::Response;
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use std::collections::VecDeque;
use std::fmt::Formatter;
use std::ops::DerefMut;
use std::ops::RangeInclusive;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use tokio::io::AsyncReadExt;

pub mod models;

pub const API_VERSION: &str = "6.0-preview";

pub mod raw {
    use super::*;

    /// Creates a file container for the new artifact in the remote blob storage/file service.
    ///
    /// Returns the response from the Artifact Service if the file container was successfully
    /// create.
    #[context("Failed to create a file container for the new  artifact `{}`.", artifact_name.as_ref())]
    pub async fn create_container(
        json_client: &reqwest::Client,
        artifact_url: Url,
        artifact_name: impl AsRef<str>,
    ) -> Result<CreateArtifactResponse> {
        let body = CreateArtifactRequest::new(artifact_name.as_ref(), None);
        //
        // dbg!(&self.json_client);
        // dbg!(serde_json::to_string(&body)?);
        let request = json_client.post(artifact_url).json(&body).build()?;

        // dbg!(&request);
        // TODO retry
        let response = json_client.execute(request).await?;
        // dbg!(&response);
        // let status = response.status();
        check_response_json(response, |status, err| match status {
            StatusCode::FORBIDDEN => err.context(
                "Artifact storage quota has been hit. Unable to upload any new artifacts.",
            ),
            StatusCode::BAD_REQUEST => err.context(format!(
                "Server rejected the request. Is the artifact name {} valid?",
                artifact_name.as_ref()
            )),
            _ => err,
        })
        .await
    }


    pub async fn upload_file_chunk(
        client: &reqwest::Client,
        upload_url: Url,
        body: impl Into<Body>,
        range: ContentRange,
        remote_path: impl AsRef<Path>,
    ) -> Result<usize> {
        use path_slash::PathExt;
        let body = body.into();
        let response = client
            .put(upload_url)
            .query(&[("itemPath", remote_path.as_ref().to_slash_lossy())])
            .header(reqwest::header::CONTENT_LENGTH, range.len())
            .header(reqwest::header::CONTENT_RANGE, &range)
            .body(body)
            .send()
            .await?;

        check_response(response, |_, e| e).await?;
        Ok(range.len())
    }

    #[context("Failed to upload the file '{}' to path '{}'.", local_path.as_ref().display(), remote_path.as_ref().display())]
    pub async fn upload_file(
        client: &reqwest::Client,
        upload_url: Url,
        local_path: impl AsRef<Path>,
        remote_path: impl AsRef<Path>,
    ) -> Result<usize> {
        let file = tokio::fs::File::open(local_path.as_ref()).await?;
        // TODO [mwu] note that metadata can lie about file size, e.g. named pipes on Linux
        let chunk_size = 8 * 1024 * 1024;
        let len = file.metadata().await?.len() as usize;
        println!("Will upload file {} of size {}", local_path.as_ref().display(), len);
        if len < chunk_size && len > 0 {
            let range = ContentRange::whole(len as usize);
            upload_file_chunk(client, upload_url.clone(), file, range, &remote_path).await
        } else {
            let mut chunks = stream_file_in_chunks(file, chunk_size).boxed();
            let mut current_position = 0;
            loop {
                let chunk = match chunks.try_next().await? {
                    Some(chunk) => chunk,
                    None => break,
                };

                let read_bytes = chunk.len();
                let range = ContentRange {
                    range: current_position..=current_position + read_bytes.saturating_sub(1),
                    total: Some(len),
                };
                upload_file_chunk(client, upload_url.clone(), chunk, range, &remote_path).await?;
                current_position += read_bytes;
            }
            Ok(current_position)
        }
    }
}

pub fn stream_file_in_chunks(
    file: tokio::fs::File,
    chunk_size: usize,
) -> impl futures::Stream<Item = Result<Bytes>> + Send {
    futures::stream::try_unfold(file, async move |mut file| {
        let mut buffer = BytesMut::with_capacity(chunk_size);
        while file.read_buf(&mut buffer).await? > 0 && buffer.len() < chunk_size {}
        if buffer.is_empty() {
            Ok::<_, anyhow::Error>(None)
        } else {
            Ok(Some((buffer.freeze(), file)))
        }
    })
}


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

        let base_builder =
            ClientBuilder::new().default_headers(headers).user_agent(crate::USER_AGENT);
        f(base_builder).build().anyhow_err()
    }
}

#[derive(Clone, Debug)]
pub struct FileToUpload {
    /// Absolute path in the local filesystem.
    pub local_path:  PathBuf,
    /// Relative path within the artifact container. Does not include the leading segment with the
    /// artifact name.
    pub remote_path: PathBuf,
}

impl FileToUpload {
    pub fn new_under_root(
        root_path: impl AsRef<Path>,
        local_path: impl Into<PathBuf>,
    ) -> Result<Self> {
        let local_path = local_path.into();
        Ok(FileToUpload {
            remote_path: local_path
                .strip_prefix(&root_path)
                .context(format!(
                    "Failed to strip prefix {} from path {}",
                    root_path.as_ref().display(),
                    local_path.display()
                ))?
                .to_path_buf(),
            local_path,
        })
    }
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

    pub fn len(&self) -> usize {
        1 + self.range.end() - self.range.start()
    }
}

impl TryFrom<ContentRange> for HeaderValue {
    type Error = InvalidHeaderValue;

    fn try_from(value: ContentRange) -> std::result::Result<Self, Self::Error> {
        value.to_string().try_into()
    }
}

impl TryFrom<&ContentRange> for HeaderValue {
    type Error = InvalidHeaderValue;

    fn try_from(value: &ContentRange) -> std::result::Result<Self, Self::Error> {
        value.to_string().try_into()
    }
}

impl Display for ContentRange {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "bytes {}-{}/{}",
            self.range.start(),
            self.range.end(),
            self.total.map_or(String::from("*"), |total| total.to_string())
        )
    }
}

pub struct UploadWorker {}

#[derive(Debug)]
pub struct ArtifactHandler {
    pub json_client:   Client,
    pub binary_client: Client,
    pub artifact_name: String,
    pub artifact_url:  Url,
    pub upload_url:    Url,
    pub total_size:    std::sync::atomic::AtomicUsize,
}

impl ArtifactHandler {
    pub async fn new(context: &Context, artifact_name: impl AsRef<str>) -> Result<Self> {
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

        let container =
            raw::create_container(&json_client, context.artifact_url()?, &artifact_name).await?;
        Ok(ArtifactHandler {
            json_client,
            binary_client,
            artifact_name: artifact_name.as_ref().into(),
            artifact_url: context.artifact_url()?,
            upload_url: container.file_container_resource_url,
            total_size: default(),
        })
    }

    pub fn uploader(&self) -> FileUploader {
        FileUploader {
            url:           self.upload_url.clone(),
            client:        self.binary_client.clone(),
            artifact_name: PathBuf::from(&self.artifact_name),
        }
    }

    /// Concurrently upload all of the files in chunks.
    pub async fn upload_artifact_to_file_container(
        &self,
        files_to_upload: impl futures::Stream<Item = FileToUpload> + Send + 'static,
        options: &UploadOptions,
    ) -> Result {
        println!(
            "File Concurrency: {}, and Chunk Size: {}.  URL: {}",
            options.file_concurrency, options.chunk_size, self.upload_url
        );

        let (work_tx, work_rx) = flume::unbounded();
        let (result_tx, result_rx) = flume::unbounded();

        tokio::task::spawn(async move {
            println!("Spawned the file discovery worker.");
            files_to_upload
                .inspect(|f| println!("File {} discovered for upload.", f.local_path.display()))
                .map(Ok)
                .forward(work_tx.into_sink())
                .await
                .unwrap();
            println!("File discovery complete.");
        });

        for task_index in 0..options.file_concurrency {
            println!("Preparing file upload worker #{}.", task_index);
            let continue_on_error = options.continue_on_error; // TODO
            let uploader = self.uploader();
            let mut job_receiver = work_rx.clone().into_stream();
            let result_sender = result_tx.clone();

            let task = async move {
                println!("Upload worker #{} has spawned.", task_index);
                while let Some(file_to_upload) = job_receiver.next().await {
                    println!(
                        "#{}: Will upload {} to {}.",
                        task_index,
                        &file_to_upload.local_path.display(),
                        &file_to_upload.remote_path.display()
                    );
                    let result = uploader.upload_file(&file_to_upload).await;
                    println!(
                        "Uploading result for {}: {:?}",
                        &file_to_upload.local_path.display(),
                        result
                    );
                    result_sender.send(result).unwrap();
                }
                println!("Upload worker #{} finished.", task_index);
            };

            println!("Spawning the upload worker #{}.", task_index);
            tokio::spawn(task);
        }

        drop(result_tx);

        let collect_results = result_rx
            .into_stream()
            .fold(0, |len_so_far, result| ready(len_so_far + result.total_size));

        let uploaded = collect_results.await;
        println!("Uploaded in total {} bytes.", uploaded);
        self.total_size.fetch_add(uploaded, Ordering::SeqCst);
        Ok(())
    }

    #[context("Failed to finalize upload of the artifact `{}`.", artifact_name)]
    pub async fn patch_artifact_size(
        &self,
        artifact_name: &str,
    ) -> Result<PatchArtifactSizeResponse> {
        println!("Patching the artifact `{}` size.", artifact_name);
        let artifact_url = self.artifact_url.clone();

        let patch_request = self
            .json_client
            .patch(artifact_url.clone())
            .query(&[("artifactName", artifact_name)]) // OsStr can be passed here, fails runtime
            .json(&PatchArtifactSize { size: self.total_size.load(Ordering::SeqCst) });

        // TODO retry
        let response = patch_request.send().await?;
        Ok(response.json().await?)
    }
}

pub async fn check_response_json<T: DeserializeOwned>(
    response: Response,
    additional_context: impl FnOnce(StatusCode, anyhow::Error) -> anyhow::Error,
) -> Result<T> {
    let data = check_response(response, additional_context).await?;
    serde_json::from_slice(data.as_ref()).context(anyhow!(
        "Failed to deserialize response body as {}. Body was: {:?}",
        std::any::type_name::<T>(),
        data,
    ))
}
pub async fn check_response(
    response: Response,
    additional_context: impl FnOnce(StatusCode, anyhow::Error) -> anyhow::Error,
) -> Result<Bytes> {
    // dbg!(&response);
    let status = response.status();
    if !status.is_success() {
        let mut err = anyhow!("Server replied with status {}.", status);

        let body = response
            .bytes()
            .await
            .map_err(|e| anyhow!("Also failed to obtain the response body: {}", e))?;

        if let Ok(body_text) = std::str::from_utf8(body.as_ref()) {
            err = err.context(format!("Error response body was: {}", body_text));
        }

        let err = additional_context(status, err);
        Err(err)
    } else {
        response.bytes().await.context("Failed to read the response body.")
    }
}

pub struct FileUploader {
    pub url:           Url,
    pub client:        Client,
    pub artifact_name: PathBuf,
}

impl FileUploader {
    pub async fn upload_file(&self, file_to_upload: &FileToUpload) -> UploadResult {
        let uploading_res = raw::upload_file(
            &self.client,
            self.url.clone(),
            &file_to_upload.local_path,
            self.artifact_name.join(&file_to_upload.remote_path),
        )
        .await;
        match uploading_res {
            Ok(len) => UploadResult {
                is_success:             true,
                total_size:             len,
                successful_upload_size: len,
            },
            Err(e) => {
                println!("Upload failed: {:?}", e);
                UploadResult {
                    is_success:             false,
                    total_size:             0,
                    successful_upload_size: 0,
                }
            }
        }
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

pub fn discover_and_feed(root_path: impl AsRef<Path>, sender: Sender<FileToUpload>) -> Result {
    walkdir::WalkDir::new(&root_path).into_iter().try_for_each(|entry| {
        let entry = entry?;
        if entry.file_type().is_file() {
            let file = FileToUpload::new_under_root(&root_path, entry.path())?;
            sender
                .send(file)
                .context("Stopping discovery in progress, because all listeners were dropped.")?;
        };
        Ok(())
    })
}

pub fn discover_recursive(
    root_path: impl Into<PathBuf>,
) -> impl Stream<Item = FileToUpload> + Send {
    let root_path = root_path.into();

    let (tx, rx) = flume::unbounded();
    tokio::task::spawn_blocking(move || discover_and_feed(root_path, tx));
    rx.into_stream()
}

pub async fn upload_artifact(
    file_provider: impl futures_util::Stream<Item = FileToUpload> + Send + 'static,
    artifact_name: impl AsRef<str>,
) -> Result {
    let options = UploadOptions {
        chunk_size:        8000000,
        file_concurrency:  10,
        continue_on_error: true,
    };

    let context = Context::new()?;
    let handler = ArtifactHandler::new(&context, artifact_name.as_ref()).await?;
    handler.upload_artifact_to_file_container(file_provider, &options).await?;
    handler.patch_artifact_size(artifact_name.as_ref()).await?;
    Ok(())
}

// pub async fn upload_path(path: impl AsRef<Path>) -> Result {
//     let filename = path.as_ref().file_name().unwrap();
//     let name = filename.to_str().unwrap();
//
//     let options = UploadOptions {
//         chunk_size:        8_000_000,
//         file_concurrency:  10,
//         continue_on_error: true,
//     };
//
//     let files = vec![FileToUpload {
//         local_path:  path.as_ref().to_path_buf(),
//         remote_path: PathBuf::from(filename).join(name),
//     }];
//
//     let context = Context::new()?;
//     let mut handler = ArtifactHandler::new(&context, name)?;
//     handler
//         .upload_artifact_to_file_container(&container.file_container_resource_url, files,
// &options)         .await?;
//     handler.patch_artifact_size(name).await?;
//     Ok(())
// }

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
    use wiremock::matchers::method;
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_artifact_upload() -> Result {
        let mock_server = MockServer::start().await;

        let text = r#"{"containerId":11099678,"size":-1,"signedContent":null,"fileContainerResourceUrl":"https://pipelines.actions.githubusercontent.com/VYS7uSE1JB12MkavBOHvD6nounefzg1s5vHmQvfbiLmuvFuM6c/_apis/resources/Containers/11099678","type":"actions_storage","name":"SomeFile","url":"https://pipelines.actions.githubusercontent.com/VYS7uSE1JB12MkavBOHvD6nounefzg1s5vHmQvfbiLmuvFuM6c/_apis/pipelines/1/runs/75/artifacts?artifactName=SomeFile","expiresOn":"2022-01-29T04:07:24.5807079Z","items":null}"#;
        mock_server
            .register(
                Mock::given(method("POST"))
                    .respond_with(ResponseTemplate::new(StatusCode::CREATED).set_body_string(text)),
            )
            .await;

        mock_server
            .register(
                Mock::given(method("PUT"))
                    .respond_with(ResponseTemplate::new(StatusCode::NOT_FOUND)),
            )
            .await;

        std::env::set_var("ACTIONS_RUNTIME_URL", mock_server.uri());
        std::env::set_var("ACTIONS_RUNTIME_TOKEN", "password123");
        std::env::set_var("GITHUB_RUN_ID", "12");

        let path_to_upload = "Cargo.toml";

        let file_to_upload = FileToUpload {
            local_path:  PathBuf::from(path_to_upload),
            remote_path: PathBuf::from(path_to_upload),
        };

        upload_artifact(futures::stream::once(ready(file_to_upload)), "MyCargoArtifact").await?;
        // artifacts::upload_path(path_to_upload).await?;
        Ok(())
        //let client = reqwest::Client::builder().default_headers().
    }


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
