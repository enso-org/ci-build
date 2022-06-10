use crate::prelude::*;

use derivative::Derivative;
use ide_ci::actions::artifacts;
use ide_ci::cache;
use ide_ci::cache::Cache;
use octocrab::models::repos::Asset;
use tokio::process::Child;

use crate::source::CiRunSource;
use crate::source::ExternalSource;
use crate::source::GetTargetJob;
use crate::source::OngoingCiRunSource;
use crate::source::ReleaseSource;
use crate::source::Source;

pub mod backend;
pub mod engine;
pub mod gui;
pub mod ide;
pub mod project_manager;
pub mod wasm;

pub use backend::Backend;
pub use engine::Engine;
pub use gui::Gui;
pub use ide::Ide;
pub use project_manager::ProjectManager;
pub use wasm::Wasm;

// FIXME: this works for Project Manager bundle-style archives only, not all.
pub fn path_to_extract() -> Option<PathBuf> {
    Some("enso".into())
}

/// A built target, contained under a single directory.
///
/// The `AsRef<Path>` trait must return that directory path.
pub trait IsArtifact: Clone + AsRef<Path> + Sized + Send + Sync + 'static {}

/// Plain artifact is just a folder with... things.
#[derive(Clone, Derivative)]
#[derivative(Debug)]
pub struct PlainArtifact<T> {
    /// Directory path.
    pub path:    PathBuf,
    /// Phantom, so we can tell artifacts of different projects apart.
    #[derivative(Debug = "ignore")]
    pub phantom: PhantomData<T>,
}

impl<T> AsRef<Path> for PlainArtifact<T> {
    fn as_ref(&self) -> &Path {
        self.path.as_path()
    }
}

impl<T: Clone + Send + Sync + 'static> IsArtifact for PlainArtifact<T> {}

impl<T> PlainArtifact<T> {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into(), phantom: default() }
    }

    fn from_existing(path: impl AsRef<Path>) -> BoxFuture<'static, Result<Self>>
    where T: Send + Sync + 'static {
        ready(Ok(Self::new(path.as_ref()))).boxed()
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
    fn artifact_name(&self) -> String;

    /// Create a full artifact description from an on-disk representation.
    fn adapt_artifact(self, path: impl AsRef<Path>) -> BoxFuture<'static, Result<Self::Artifact>>;

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
        let this = self.clone();
        let span = info_span!("Getting artifact from an external source");
        match source {
            ExternalSource::OngoingCiRun(OngoingCiRunSource { artifact_name }) => async move {
                ide_ci::actions::artifacts::retrieve_compressed_directory(
                    artifact_name,
                    &destination,
                )
                .await?;
                this.adapt_artifact(destination).await
            }
            .boxed(),
            ExternalSource::CiRun(ci_run) => self.download_artifact(ci_run, destination, cache),
            ExternalSource::LocalFile(source_path) => async move {
                ide_ci::fs::mirror_directory(source_path, &destination).await?;
                this.adapt_artifact(destination).await
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
        cache: Cache,
    ) -> BoxFuture<'static, Result<Self::Artifact>> {
        let CiRunSource { run_id, artifact_name, repository, octocrab } = ci_run;
        let span = info_span!("Downloading CI Artifact.", %artifact_name, %repository, target = output_path.as_str());
        let this = self.clone();
        async move {
            let artifact =
                repository.find_artifact_by_name(&octocrab, run_id, &artifact_name).await?;
            info!("Will download artifact: {:#?}", artifact);
            let artifact_to_get = cache::artifact::ExtractedArtifact {
                client: octocrab.clone(),
                key:    cache::artifact::Key { artifact_id: artifact.id, repository },
            };
            let artifact = cache.get(artifact_to_get).await?;
            let inner_archive_path =
                artifact.join(&artifact_name).with_appended_extension("tar.gz");
            ide_ci::archive::extract_to(&inner_archive_path, &output_path).await?;
            this.adapt_artifact(output_path).await
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
        let this = self.clone();
        async move {
            let ReleaseSource { asset_id, octocrab, repository } = &source;
            let archive_source = repository.download_asset_job(octocrab, *asset_id);
            let extract_job = cache::archive::ExtractedArchive {
                archive_source,
                path_to_extract: path_to_extract(),
            };
            let directory = cache.get(extract_job).await?;
            ide_ci::fs::remove_if_exists(&destination)?;
            ide_ci::fs::symlink_auto(&directory, &destination)?;
            this.adapt_artifact(destination).await
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
    use crate::project::backend::Backend;
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

        Backend { target_os: TARGET_OS }
            .get_external(source, r"C:\temp\pm".into(), Cache::new_default().await?)
            .await?;
        Ok(())
    }
}
