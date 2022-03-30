use crate::actions::artifacts::models::PatchArtifactSizeResponse;
use crate::prelude::*;
use anyhow::Context;
use reqwest::Client;
use std::collections::VecDeque;
use std::ops::DerefMut;
use std::sync::atomic::Ordering;
use std::sync::Mutex;

use crate::actions::artifacts::raw;
use crate::actions::artifacts::run_session::SessionClient;
use crate::global;


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

#[derive(Debug)]
pub struct ArtifactUploader {
    pub client:        SessionClient,
    pub artifact_name: String,
    pub upload_url:    Url,
    pub total_size:    std::sync::atomic::AtomicUsize,
}

impl ArtifactUploader {
    pub async fn new(client: SessionClient, artifact_name: impl Into<String>) -> Result<Self> {
        let artifact_name = artifact_name.into();
        let container = client.create_container(&artifact_name).await?;
        info!("Created a container {} for artifact '{}'.", container.container_id, artifact_name);
        Ok(Self {
            client,
            artifact_name,
            upload_url: container.file_container_resource_url,
            total_size: default(),
        })
    }


    pub fn uploader(&self, options: &UploadOptions) -> FileUploader {
        FileUploader {
            url:           self.upload_url.clone(),
            client:        self.client.upload_client.clone(),
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
        debug!(
            "File Concurrency: {}, and Chunk Size: {}.  URL: {}",
            options.file_concurrency, options.chunk_size, self.upload_url
        );

        let (work_tx, work_rx) = flume::unbounded();
        let (result_tx, result_rx) = flume::unbounded();

        tokio::task::spawn(async move {
            debug!("Spawned the file discovery worker.");
            files_to_upload
                .inspect(|f| debug!("File {} discovered for upload.", f.local_path.display()))
                .map(Ok)
                .forward(work_tx.into_sink())
                .await
                .unwrap();
            debug!("File discovery complete.");
        });

        for task_index in 0..options.file_concurrency {
            debug!("Preparing file upload worker #{}.", task_index);
            let _continue_on_error = options.continue_on_error; // TODO
            let uploader = self.uploader(options);
            let mut job_receiver = work_rx.clone().into_stream();
            let result_sender = result_tx.clone();

            let task = async move {
                debug!("Upload worker #{} has spawned.", task_index);
                while let Some(file_to_upload) = job_receiver.next().await {
                    // debug!(
                    //     "#{}: Will upload {} to {}.",
                    //     task_index,
                    //     &file_to_upload.local_path.display(),
                    //     &file_to_upload.remote_path.display()
                    // );
                    let result = uploader.upload_file(&file_to_upload).await;
                    // debug!(
                    //     "Uploading result for {}: {:?}",
                    //     &file_to_upload.local_path.display(),
                    //     result
                    // );
                    result_sender.send(result).unwrap();
                }
                debug!("Upload worker #{} finished.", task_index);
                Ok(())
            };

            debug!("Spawning the upload worker #{}.", task_index);
            global::spawn("uploader", task);
        }

        drop(result_tx);

        let collect_results = result_rx
            .into_stream()
            .fold(0, |len_so_far, result| ready(len_so_far + result.total_size));

        let uploaded = collect_results.await;
        debug!("Uploaded in total {} bytes.", uploaded);
        self.total_size.fetch_add(uploaded, Ordering::SeqCst);
        Ok(())
    }

    pub async fn patch_artifact_size(&self) -> Result<PatchArtifactSizeResponse> {
        let total_size = self.total_size.load(Ordering::SeqCst);
        self.client.patch_artifact_size(&self.artifact_name, total_size).await
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
                debug!("Upload failed: {:?}", e);
                UploadResult {
                    is_success:             false,
                    total_size:             0,
                    successful_upload_size: 0,
                }
            }
        }
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
    pub fn new_in_root(path: impl Into<PathBuf>) -> Result<Self> {
        let local_path = path.into();
        let remote_path = local_path.file_name().map(into).ok_or_else(|| {
            anyhow!("Path {} does not contain a valid filename.", local_path.display())
        })?;
        Ok(Self { local_path, remote_path })
    }

    pub fn new_relative(
        root_path: impl AsRef<Path>,
        local_path: impl Into<PathBuf>,
    ) -> Result<Self> {
        let local_path = local_path.into();
        Ok(FileToUpload {
            remote_path: local_path
                .strip_prefix(&root_path)
                .context(format!(
                    "Failed to strip prefix {} from path {}.",
                    root_path.as_ref().display(),
                    local_path.display()
                ))?
                .to_path_buf(),
            local_path,
        })
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
