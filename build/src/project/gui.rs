use crate::prelude::*;
use futures_util::future::try_join;
use ide_ci::ok_ready_boxed;

use crate::ide::web::IdeDesktop;
use crate::paths::generated::RepoRoot;
use crate::project::Context;
use crate::project::IsTarget;
use crate::project::IsWatchable;
use crate::project::IsWatcher;
use crate::project::PerhapsWatched;
use crate::project::PlainArtifact;
use crate::project::Wasm;
use crate::source::BuildTargetJob;
use crate::source::GetTargetJob;
use crate::source::Source;
use crate::source::WatchTargetJob;
use crate::source::WithDestination;
use crate::BoxFuture;

pub type Artifact = PlainArtifact<Gui>;

#[derive(derivative::Derivative)]
#[derivative(Debug)]
pub struct BuildInput {
    #[derivative(Debug(format_with = "std::fmt::Display::fmt"))]
    pub repo_root:  RepoRoot,
    #[derivative(Debug = "ignore")]
    pub wasm:       GetTargetJob<Wasm>,
    /// BoxFuture<'static, Result<wasm::Artifact>>,
    #[derivative(Debug = "ignore")]
    pub build_info: BoxFuture<'static, Result<BuildInfo>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Gui;

#[async_trait]
impl IsTarget for Gui {
    type BuildInput = BuildInput;
    type Artifact = Artifact;

    fn artifact_name(&self) -> String {
        "gui".into()
    }

    fn adapt_artifact(self, path: impl AsRef<Path>) -> BoxFuture<'static, Result<Self::Artifact>> {
        Artifact::from_existing(path)
    }

    fn build_internal(
        &self,
        context: Context,
        job: BuildTargetJob<Self>,
    ) -> BoxFuture<'static, Result<Self::Artifact>> {
        let WithDestination { inner, destination } = job;
        async move {
            let ide = IdeDesktop::new(&inner.repo_root.app.ide_desktop);
            let wasm = Wasm.get(context, inner.wasm);
            ide.build_content(wasm, &inner.build_info.await?, &destination).await?;
            Ok(Artifact::new(destination))
        }
        .boxed()
    }
}

pub struct Watcher {
    pub wasm: PerhapsWatched<Wasm>,
    pub web:  crate::project::Watcher<Gui, crate::ide::web::Watcher>,
}

impl AsRef<Artifact> for Watcher {
    fn as_ref(&self) -> &Artifact {
        &self.web.artifact
    }
}

impl IsWatcher<Gui> for Watcher {
    fn wait_for_finish(&mut self) -> BoxFuture<Result> {
        let Self { web, wasm } = self;
        try_join(wasm.wait_ok(), IsWatcher::wait_for_finish(web)).void_ok().boxed()
    }
}

impl IsWatchable for Gui {
    type Watcher = Watcher;
    type WatchInput = <Wasm as IsWatchable>::WatchInput;

    // fn setup_watcher(
    //     &self,
    //     build_input: Self::BuildInput,
    //     watch_input: Self::WatchInput,
    //     output_path: impl AsRef<Path> + Send + Sync + 'static,
    // ) -> BoxFuture<'static, Result<Self::Watcher>> {
    //     async move {
    //         let BuildInput { build_info, repo_root, wasm } = build_input;
    //         let ide = IdeDesktop::new(&repo_root.app.ide_desktop);
    //         let watch_process = ide.watch_content(wasm, &build_info.await?).await?;
    //         let artifact = Self::Artifact::from_existing(output_path).await?;
    //         Ok(Self::Watcher { watch_process, artifact })
    //     }
    //     .boxed()
    // }

    fn watch(
        &self,
        context: Context,
        job: WatchTargetJob<Self>,
    ) -> BoxFuture<'static, Result<Self::Watcher>> {
        let WatchTargetJob { watch_input, build: WithDestination { inner, destination } } = job;
        let BuildInput { build_info, repo_root, wasm } = inner;
        let perhaps_watched_wasm = perhaps_watch(Wasm, context.clone(), wasm, watch_input);
        let ide = IdeDesktop::new(&repo_root.app.ide_desktop);
        async move {
            let perhaps_watched_wasm = perhaps_watched_wasm.await?;
            let wasm_artifacts = ok_ready_boxed(perhaps_watched_wasm.as_ref().clone());
            let watch_process = ide.watch_content(wasm_artifacts, &build_info.await?).await?;
            let artifact = Self::Artifact::from_existing(destination).await?;
            let web_watcher = crate::project::Watcher { watch_process, artifact };
            Ok(Self::Watcher { wasm: perhaps_watched_wasm, web: web_watcher })
        }
        .boxed()
    }
}

pub fn perhaps_watch<T: IsWatchable>(
    target: T,
    context: Context,
    job: GetTargetJob<T>,
    watch_input: T::WatchInput,
) -> BoxFuture<'static, Result<PerhapsWatched<T>>> {
    match job.inner {
        Source::BuildLocally(local) => target
            .watch(context, WatchTargetJob {
                watch_input,
                build: WithDestination { inner: local, destination: job.destination },
            })
            .map_ok(PerhapsWatched::Watched)
            .boxed(),
        Source::External(external) => target
            .get_external(context, WithDestination {
                inner:       external,
                destination: job.destination,
            })
            .map_ok(PerhapsWatched::Static)
            .boxed(),
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildInfo {
    pub commit:         String,
    pub version:        Version,
    pub engine_version: Version,
    pub name:           String,
}
