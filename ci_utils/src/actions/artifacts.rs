use crate::prelude::*;

use crate::actions::artifacts::context::Context;
use crate::actions::artifacts::models::PatchArtifactSizeResponse;
use crate::actions::artifacts::run_session::SessionClient;

use anyhow::Context as Trait_anyhow_Context;
use flume::Sender;
use reqwest::Client;
use serde::de::DeserializeOwned;
use std::collections::VecDeque;
use std::ops::DerefMut;
use std::sync::atomic::Ordering;
use std::sync::Mutex;

pub mod context;
pub mod download;
pub mod models;
pub mod raw;
pub mod run_session;
mod upload;

pub const API_VERSION: &str = "6.0-preview";

#[derive(Clone, Debug)]
pub struct FileToUpload {
    /// Absolute path in the local filesystem.
    pub local_path:  PathBuf,
    /// Relative path within the artifact container. Does not include the leading segment with the
    /// artifact name.
    pub remote_path: PathBuf,
}

impl FileToUpload {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self> {
        let local_path = path.into();
        let remote_path = local_path.file_name().map(into).ok_or_else(|| {
            anyhow!("Path {} does not contain a valid filename.", local_path.display())
        })?;
        Ok(Self { local_path, remote_path })
    }

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

impl Default for UploadOptions {
    fn default() -> Self {
        UploadOptions {
            chunk_size:        8 * 1024 * 1024,
            file_concurrency:  10,
            continue_on_error: true,
        }
    }
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


#[derive(Debug)]
pub struct ArtifactHandler {
    pub run:           SessionClient,
    pub artifact_name: String,
    pub upload_url:    Url,
    pub total_size:    std::sync::atomic::AtomicUsize,
}

impl ArtifactHandler {
    pub async fn new(context: &Context, artifact_name: impl Into<String>) -> Result<Self> {
        let artifact_name = artifact_name.into();
        let run = SessionClient::new(context)?;
        let container = run.create_container(&artifact_name).await?;
        println!(
            "Created a container {} for artifact '{}'.",
            container.container_id, artifact_name
        );
        Ok(ArtifactHandler {
            run,
            artifact_name,
            upload_url: container.file_container_resource_url,
            total_size: default(),
        })
    }

    pub fn uploader(&self, options: &UploadOptions) -> FileUploader {
        FileUploader {
            url:           self.upload_url.clone(),
            client:        self.run.binary_client.clone(),
            artifact_name: PathBuf::from(&self.artifact_name),
            chunk_size:    options.chunk_size,
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
            let _continue_on_error = options.continue_on_error; // TODO
            let uploader = self.uploader(options);
            let mut job_receiver = work_rx.clone().into_stream();
            let result_sender = result_tx.clone();

            let task = async move {
                println!("Upload worker #{} has spawned.", task_index);
                while let Some(file_to_upload) = job_receiver.next().await {
                    // println!(
                    //     "#{}: Will upload {} to {}.",
                    //     task_index,
                    //     &file_to_upload.local_path.display(),
                    //     &file_to_upload.remote_path.display()
                    // );
                    let result = uploader.upload_file(&file_to_upload).await;
                    // println!(
                    //     "Uploading result for {}: {:?}",
                    //     &file_to_upload.local_path.display(),
                    //     result
                    // );
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

    pub async fn patch_artifact_size(&self) -> Result<PatchArtifactSizeResponse> {
        let total_size = self.total_size.load(Ordering::SeqCst);
        self.run.patch_artifact_size(&self.artifact_name, total_size).await
    }
}

pub struct FileUploader {
    pub url:           Url,
    pub client:        Client,
    pub artifact_name: PathBuf,
    pub chunk_size:    usize,
}

impl FileUploader {
    pub async fn upload_file(&self, file_to_upload: &FileToUpload) -> UploadResult {
        let uploading_res = raw::upload_file(
            &self.client,
            self.chunk_size,
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
    options: UploadOptions,
) -> Result {
    let context = Context::new_from_env()?;
    let handler = ArtifactHandler::new(&context, artifact_name.as_ref()).await?;
    handler.upload_artifact_to_file_container(file_provider, &options).await?;
    handler.patch_artifact_size().await?;
    Ok(())
}

pub fn single_file_provider(path: impl Into<PathBuf>) -> Result<impl Stream<Item = FileToUpload>> {
    let file = FileToUpload::new(path)?;
    Ok(futures::stream::iter([file]))
}


// pub async fn upload_single_file(
//     path: impl Into<PathBuf>
// ) -> Result {
//     let file = FileToUpload::new(path)?;
//     let artifact_name = file.remote_path.as_str().to_owned();
//     let provider = futures::stream::iter([file]);
//     upload_artifact(provider, artifact_name).await
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
