use crate::prelude::*;

use ide_ci::models::config::RepoContext;
use octocrab::models::RunId;

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

    /// Upload artifact to the current GitHub Actions run.
    fn upload_artifact(
        &self,
        output: impl Future<Output = Result<Self::Output>> + Send + 'static,
    ) -> BoxFuture<'static, Result> {
        let name = self.artifact_name().to_string();
        async move {
            info!("Starting upload of {name}.");
            let output = output.await?;
            ide_ci::actions::artifacts::upload_directory(output.as_ref(), &name).await?;
            info!("Completed upload of {name}.");
            Ok(())
        }
        .boxed()
    }

    fn download_artifact(
        &self,
        ci_run: CiRunSource,
        output_path: impl AsRef<Path> + Send + Sync + 'static,
    ) -> BoxFuture<'static, Result<Self::Output>> {
        let CiRunSource { run_id, artifact_name, repository, octocrab } = ci_run;
        let artifact_name = artifact_name.unwrap_or_else(|| self.artifact_name().to_string());
        async move {
            let artifact =
                repository.find_artifact_by_name(&octocrab, run_id, &artifact_name).await?;
            info!("Will download artifact: {:#?}", artifact);
            ide_ci::fs::reset_dir(&output_path)?;
            repository
                .download_and_unpack_artifact(&octocrab, artifact.id, output_path.as_ref())
                .await?;
            Self::Output::from_existing(output_path).await
        }
        .boxed()
    }
}

pub struct CiRunSource {
    pub octocrab:      Octocrab,
    pub repository:    RepoContext,
    pub run_id:        RunId,
    pub artifact_name: Option<String>,
}
