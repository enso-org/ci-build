use crate::prelude::*;

use ide_ci::actions::artifacts;
use ide_ci::cache;
use ide_ci::cache::Cache;
use octocrab::models::repos::Asset;
use tokio::process::Child;

use crate::source::CiRunSource;
use crate::source::ExternalSource;
use crate::source::GetTargetJob;
use crate::source::ReleaseSource;
use crate::source::Source;

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

/// Build targets, like GUI or Project Manager.
///
/// Built target generates artifacts that can be stored as a release asset or CI run artifacts.
pub trait IsTarget: Clone + Debug + Sized + Send + Sync + 'static {
    /// All the data needed to build this target that are not placed in `self`.
    type BuildInput: Debug + Send + 'static;

    /// A location-like value with the directory where the artifacts are placed.
    type Artifact: IsArtifact;

    /// Identifier used when uploading build artifacts to run.
    ///
    /// Note that this is not related to the assets name in the release.
    fn artifact_name(&self) -> &str;


    fn get(
        &self,
        job: GetTargetJob<Self>,
        cache: Cache,
    ) -> BoxFuture<'static, Result<Self::Artifact>> {
        match job.source {
            Source::BuildLocally(inputs) => self.build_locally(inputs, job.destination),
            Source::External(external) => self.get_external(external, job.destination, cache),
        }
    }

    /// Produce an artifact from the external resource reference.
    fn get_external(
        &self,
        source: ExternalSource,
        destination: PathBuf,
        cache: Cache,
    ) -> BoxFuture<'static, Result<Self::Artifact>> {
        let span = info_span!("Getting artifact from an external source");
        match source {
            ExternalSource::OngoingCiRun => {
                let artifact_name = self.artifact_name().to_string();
                async move {
                    ide_ci::actions::artifacts::download_single_file_artifact(
                        artifact_name,
                        &destination,
                    )
                    .await?;
                    Self::Artifact::from_existing(destination).await
                }
                .boxed()
            }
            ExternalSource::CiRun(ci_run) => self.download_artifact(ci_run, destination),
            ExternalSource::LocalFile(source_path) => async move {
                ide_ci::fs::mirror_directory(source_path, &destination).await?;
                Self::Artifact::from_existing(destination).await
            }
            .boxed(),
            ExternalSource::Release(release) => self.download_asset(release, destination, cache),
        }
        .instrument(span)
        .boxed()
    }

    /// Produce an artifact from build inputs.
    fn build_locally(
        &self,
        input: Self::BuildInput,
        output_path: impl AsRef<Path> + Send + Sync + 'static,
    ) -> BoxFuture<'static, Result<Self::Artifact>>;

    /// Upload artifact to the current GitHub Actions run.
    fn upload_artifact(
        &self,
        output: impl Future<Output = Result<Self::Artifact>> + Send + 'static,
    ) -> BoxFuture<'static, Result> {
        let name = self.artifact_name().to_string();
        async move { artifacts::upload_compressed_directory(output.await?, name).await }.boxed()
    }

    fn download_artifact(
        &self,
        ci_run: CiRunSource,
        output_path: impl AsRef<Path> + Send + Sync + 'static,
    ) -> BoxFuture<'static, Result<Self::Artifact>> {
        let CiRunSource { run_id, artifact_name, repository, octocrab } = ci_run;
        let artifact_name = artifact_name.unwrap_or_else(|| self.artifact_name().to_string());
        let span = info_span!("Downloading CI Artifact.", %artifact_name, %repository, target = output_path.as_str());
        async move {
            let artifact =
                repository.find_artifact_by_name(&octocrab, run_id, &artifact_name).await?;
            info!("Will download artifact: {:#?}", artifact);
            ide_ci::fs::reset_dir(&output_path)?;
            repository
                .download_and_unpack_artifact(&octocrab, artifact.id, output_path.as_ref())
                .await?;

            let inner_archive_path =
                output_path.as_ref().join(&artifact_name).with_appended_extension("tar.gz");
            ide_ci::archive::extract_to(&inner_archive_path, &output_path).await?;
            ide_ci::fs::remove_if_exists(&inner_archive_path)?;
            Self::Artifact::from_existing(output_path).await
        }
        .instrument(span)
        .boxed()
    }

    fn find_asset(&self, _assets: Vec<Asset>) -> Result<Asset> {
        todo!("Not implemented for target {self:?}!")
    }

    fn download_asset(
        &self,
        source: ReleaseSource,
        destination: PathBuf,
        cache: Cache,
    ) -> BoxFuture<'static, Result<Self::Artifact>> {
        // source.asset_id
        let span = info_span!("Downloading built target from a release asset.",
            asset_id = source.asset_id.0,
            repo = %source.repository);
        async move {
            let ReleaseSource { asset_id, octocrab, repository } = &source;
            let archive_source = repository.download_asset_job(octocrab, *asset_id);
            let extract_job = cache::archive::ExtractedArchive { archive_source };
            let directory = cache.get(extract_job).await?;
            ide_ci::fs::remove_if_exists(&destination)?;
            symlink::symlink_auto(&directory, &destination)?;
            Self::Artifact::from_existing(destination).await
        }
        .instrument(span)
        .boxed()
    }
}


pub enum PerhapsWatched<T: IsWatchable> {
    Watched(T::Watcher),
    Static(T::Artifact),
}

impl<T: IsWatchable> AsRef<T::Artifact> for PerhapsWatched<T> {
    fn as_ref(&self) -> &T::Artifact {
        match self {
            PerhapsWatched::Watched(watcher) => watcher.as_ref(),
            PerhapsWatched::Static(static_artifact) => &static_artifact,
        }
    }
}

impl<T: IsWatchable> PerhapsWatched<T> {
    pub async fn wait_ok(&mut self) -> Result {
        match self {
            PerhapsWatched::Watched(watcher) => watcher.wait_ok().await,
            PerhapsWatched::Static(_) => Ok(()),
        }
    }
}

pub trait ProcessWrapper {
    fn inner(&mut self) -> &mut tokio::process::Child;

    fn wait_ok(&mut self) -> BoxFuture<Result> {
        ide_ci::extensions::child::ChildExt::wait_ok(self.inner()).boxed()
    }
    fn kill(&mut self) -> BoxFuture<Result> {
        self.inner().kill().anyhow_err().boxed()
    }
}

impl ProcessWrapper for tokio::process::Child {
    fn inner(&mut self) -> &mut Child {
        self
    }
}

/// Watcher is an ongoing process that keeps updating the artifacts to follow changes to the
/// target's source.
pub struct Watcher<Target: IsWatchable, Proc> {
    /// Where the watcher outputs artifacts.
    pub artifact:      Target::Artifact,
    /// The process performing the watch.
    ///
    /// In this case, an instance of cargo-watch.
    pub watch_process: Proc,
}

impl<Target: IsWatchable, Proc: ProcessWrapper> ProcessWrapper for Watcher<Target, Proc> {
    fn inner(&mut self) -> &mut Child {
        self.watch_process.inner()
    }
}

impl<Target: IsWatchable, Proc> AsRef<Target::Artifact> for Watcher<Target, Proc> {
    fn as_ref(&self) -> &Target::Artifact {
        &self.artifact
    }
}

impl<Target: IsWatchable, Proc: ProcessWrapper> IsWatcher<Target> for Watcher<Target, Proc> {
    fn wait_ok(&mut self) -> BoxFuture<Result> {
        self.watch_process.wait_ok()
    }
}


pub trait IsWatcher<Target: IsTarget>: AsRef<Target::Artifact> {
    fn wait_ok(&mut self) -> BoxFuture<Result>;
}

pub trait IsWatchable: IsTarget {
    type Watcher: IsWatcher<Self>;

    fn setup_watcher(
        &self,
        input: Self::BuildInput,
        output_path: impl AsRef<Path> + Send + Sync + 'static,
    ) -> BoxFuture<'static, Result<Self::Watcher>>;

    fn watch(
        &self,
        job: GetTargetJob<Self>,
        cache: Cache,
    ) -> BoxFuture<'static, Result<PerhapsWatched<Self>>> {
        match job.source {
            Source::BuildLocally(input) => {
                let watcher = self.setup_watcher(input, job.destination);
                watcher.map_ok(PerhapsWatched::Watched).boxed()
            }
            Source::External(external) => self
                .get_external(external, job.destination, cache)
                .map_ok(PerhapsWatched::Static)
                .boxed(),
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::project_manager::ProjectManager;
    use ide_ci::log::setup_logging;
    use ide_ci::models::config::RepoContext;

    #[tokio::test]
    async fn download_release() -> Result {
        setup_logging()?;
        let source = ExternalSource::Release(ReleaseSource {
            repository: RepoContext::from_str("enso-org/enso")?,
            // release: 64573522.into(),
            asset_id:   62731588.into(),
            // asset_id:   62731653.into(),
            octocrab:   Default::default(),
        });

        ProjectManager
            .get_external(source, r"C:\temp\pm".into(), Cache::new_default().await?)
            .await?;
        Ok(())
    }
}
