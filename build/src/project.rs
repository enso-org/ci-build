use crate::prelude::*;

use derivative::Derivative;
use ide_ci::actions::artifacts;
use ide_ci::cache;
use ide_ci::cache::Cache;
use ide_ci::ok_ready_boxed;
use octocrab::models::repos::Asset;
use tokio::process::Child;

use crate::source::BuildTargetJob;
use crate::source::CiRunSource;
use crate::source::ExternalSource;
use crate::source::FetchTargetJob;
use crate::source::GetTargetJob;
use crate::source::OngoingCiRunSource;
use crate::source::ReleaseSource;
use crate::source::Source;
use crate::source::WatchTargetJob;
use crate::source::WithDestination;

pub mod backend;
pub mod engine;
pub mod gui;
pub mod ide;
pub mod project_manager;
pub mod wasm;

pub use backend::Backend;
pub use gui::Gui;
pub use ide::Ide;
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

/// State available to all project-related operations.
#[derive(Clone, Derivative)]
#[derivative(Debug)]
pub struct Context {
    /// GitHub API client.
    ///
    /// If authorized, it will count API rate limits against our identity and allow operations like
    /// managing releases or downloading CI run artifacts.
    #[derivative(Debug = "ignore")]
    pub octocrab: Octocrab,

    /// Stores things like downloaded release assets to save time.
    pub cache: Cache,

    /// Whether built artifacts should be uploaded as part of CI run. Works only in CI environment.
    pub upload_artifacts: bool,
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
        context: Context,
        job: GetTargetJob<Self>,
    ) -> BoxFuture<'static, Result<Self::Artifact>> {
        let GetTargetJob { destination, inner } = job;
        match inner {
            Source::BuildLocally(inputs) =>
                self.build(context, WithDestination { inner: inputs, destination }),
            Source::External(external) =>
                self.get_external(context, WithDestination { inner: external, destination }),
        }
    }

    /// Produce an artifact from the external resource reference.
    fn get_external(
        &self,
        context: Context,
        job: FetchTargetJob,
    ) -> BoxFuture<'static, Result<Self::Artifact>> {
        let FetchTargetJob { inner: source, destination } = job;
        let this = self.clone();
        let span = debug_span!("Getting artifact from an external source");
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
            ExternalSource::CiRun(ci_run) => self.download_artifact(context, ci_run, destination),
            ExternalSource::LocalFile(source_path) => async move {
                ide_ci::fs::mirror_directory(source_path, &destination).await?;
                this.adapt_artifact(destination).await
            }
            .boxed(),
            ExternalSource::Release(release) => self.download_asset(context, release, destination),
        }
        .instrument(span)
        .boxed()
    }

    /// Produce an artifact from build inputs.
    fn build(
        &self,
        context: Context,
        job: BuildTargetJob<Self>,
    ) -> BoxFuture<'static, Result<Self::Artifact>> {
        let span = debug_span!("Building.", ?self, ?context, ?job).entered();
        let upload_artifacts = context.upload_artifacts;
        let artifact_fut = self.build_internal(context, job);
        let this = self.clone();
        async move {
            let artifact = artifact_fut.await.context(format!("Failed to build {:?}.", this))?;
            // We upload only built artifacts. There would be no point in uploading something that
            // we've just downloaded. That's why the uploading code is here.
            if upload_artifacts {
                this.perhaps_upload_artifact(&artifact).await?;
            }
            Ok(artifact)
        }
        .instrument(span.exit())
        .boxed()
    }

    fn perhaps_upload_artifact(&self, artifact: &Self::Artifact) -> BoxFuture<'static, Result> {
        let should_upload_artifact = ide_ci::actions::workflow::is_in_env();
        trace!("Got target {:?}, should it be uploaded? {}", self, should_upload_artifact);
        if should_upload_artifact {
            self.upload_artifact(ready(Ok(artifact.clone())))
        } else {
            ok_ready_boxed(())
        }
    }

    /// Produce an artifact from build inputs.
    fn build_internal(
        &self,
        context: Context,
        job: BuildTargetJob<Self>,
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
        context: Context,
        ci_run: CiRunSource,
        output_path: impl AsRef<Path> + Send + Sync + 'static,
    ) -> BoxFuture<'static, Result<Self::Artifact>> {
        let Context { octocrab, cache, upload_artifacts: _ } = context;
        let CiRunSource { run_id, artifact_name, repository } = ci_run;
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
        context: Context,
        source: ReleaseSource,
        destination: PathBuf,
    ) -> BoxFuture<'static, Result<Self::Artifact>> {
        let Context { octocrab, cache, upload_artifacts: _ } = context;
        let span = info_span!("Downloading built target from a release asset.",
            asset_id = source.asset_id.0,
            repo = %source.repository);
        let this = self.clone();
        async move {
            let ReleaseSource { asset_id, repository } = &source;
            let archive_source = repository.download_asset_job(&octocrab, *asset_id);
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
            PerhapsWatched::Watched(watcher) => watcher.wait_for_finish().await,
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
    /// For example, an instance of cargo-watch.
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

impl<Target: IsWatchable, Proc: ProcessWrapper + Send> IsWatcher<Target> for Watcher<Target, Proc> {
    fn wait_for_finish(&mut self) -> BoxFuture<Result> {
        self.watch_process.wait_ok()
    }
}

pub trait IsWatcher<Target: IsTarget>: AsRef<Target::Artifact> + Send {
    fn wait_for_finish(&mut self) -> BoxFuture<Result>;
}

pub trait IsWatchable: IsTarget {
    type Watcher: IsWatcher<Self>;
    type WatchInput: Clone + Debug + Send;

    fn watch(
        &self,
        context: Context,
        job: WatchTargetJob<Self>,
    ) -> BoxFuture<'static, Result<Self::Watcher>>;
}
