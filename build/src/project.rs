use crate::prelude::*;
use std::marker::PhantomData;

use ide_ci::actions::workflow::is_in_env;

pub mod gui;
pub mod ide;
pub mod project_manager;
pub mod wasm;

pub trait IsArtifact: Clone + AsRef<Path> + Sized + Send + Sync + 'static {
    fn from_existing(path: impl AsRef<Path>) -> BoxFuture<'static, Result<Self>>;
}

impl IsArtifact for PathBuf {
    fn from_existing(path: impl AsRef<Path>) -> BoxFuture<'static, Result<Self>> {
        ready(Ok(path.as_ref().to_path_buf())).boxed()
    }
}

/// The content, i.e. WASM, HTML, JS, assets.
pub struct Artifact(pub PathBuf);

impl From<&Path> for Artifact {
    fn from(path: &Path) -> Self {
        Artifact(path.into())
    }
}

#[derive(Clone, Debug)]
pub struct PlainArtifact<T> {
    pub path:    PathBuf,
    pub phantom: PhantomData<T>,
}

impl<T> AsRef<Path> for PlainArtifact<T> {
    fn as_ref(&self) -> &Path {
        self.path.as_path()
    }
}

impl<T: Clone + Send + Sync + 'static> IsArtifact for PlainArtifact<T> {
    fn from_existing(path: impl AsRef<Path>) -> BoxFuture<'static, Result<Self>> {
        ready(Ok(PlainArtifact::new(path.as_ref()))).boxed()
    }
}

impl<T> PlainArtifact<T> {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into(), phantom: default() }
    }
}

pub trait IsTarget: Sized {
    /// All the data needed to build this target that are not placed in `self`.
    type BuildInput: Send + 'static;

    /// A location-like value with the directory where the artifacts are placed.
    type Output: IsArtifact;

    /// Identifier used when uploading build artifacts to run.
    ///
    /// Note that this is not related to the assets name in the release.
    fn artifact_name(&self) -> &str;

    /// Produce an artifact from build inputs.
    fn build(
        &self,
        input: Self::BuildInput,
        output_path: impl AsRef<Path> + Send + Sync + 'static,
    ) -> BoxFuture<'static, Result<Self::Output>>;

    fn get(
        &self,
        source: Source,
        get_inputs: impl FnOnce() -> Result<Self::BuildInput> + Send + 'static,
        output: PathBuf,
    ) -> BoxFuture<'static, Result<Self::Output>> {
        match source {
            Source::BuildLocally => match get_inputs() {
                Ok(inputs) => self.build(inputs, output),
                Err(e) => ready(Err(e)).boxed(),
            },
            Source::OngoingCiRun => {
                let artifact_name = self.artifact_name().to_string();
                async move {
                    ide_ci::actions::artifacts::download_single_file_artifact(
                        artifact_name,
                        output.as_path(),
                    )
                    .await?;
                    Self::Output::from_existing(output).await
                }
                .boxed()
            }
            Source::LocalFile(source_dir) => async move {
                ide_ci::fs::mirror_directory(source_dir, output.as_path()).await?;
                Self::Output::from_existing(output).await
            }
            .boxed(),
        }
    }

    fn upload_artifact(
        &self,
        output: impl Future<Output = Result<Self::Output>> + Send + 'static,
    ) -> BoxFuture<'static, Result> {
        let name = self.artifact_name().to_string();
        async move {
            info!("Starting upload of {name}.");
            // Note that this will not attempt getting artifact if it does not need it.
            if is_in_env() {
                let output = output.await?;
                ide_ci::actions::artifacts::upload_directory(output.as_ref(), &name).await?;
                info!("Completed upload of {name}.");
            } else {
                warn!(
                    "Aborting upload of {name} because we are not in GitHub Actions environment."
                );
            }
            Ok(())
        }
        .boxed()
    }
}


pub enum Source {
    BuildLocally,
    OngoingCiRun,
    LocalFile(PathBuf),
}
