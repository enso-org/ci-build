// #![feature(explicit_generic_args_with_impl_trait)]
// #![feature(once_cell)]
// #![feature(exit_status_error)]
// #![feature(associated_type_defaults)]
// #![feature(is_some_with)]
// #![feature(default_free_fn)]
// #![feature(adt_const_params)]

pub use crate::prelude;

use crate::prelude::*;

use crate::args::BuildKind;
use crate::cli::arg;
use crate::cli::arg::Cli;
use crate::cli::arg::IsTargetSource;
use crate::cli::arg::Target;
use crate::paths::generated::RepoRoot;
use crate::paths::TargetTriple;
use crate::prettier;
use crate::project::gui;
use crate::project::gui::Gui;
use crate::project::ide;
use crate::project::ide::Ide;
use crate::project::project_manager;
use crate::project::project_manager::ProjectManager;
use crate::project::wasm;
use crate::project::wasm::Wasm;
use crate::project::IsTarget;
use crate::project::IsWatchable;
use crate::project::IsWatcher;
use crate::setup_octocrab;
use crate::source::CiRunSource;
use crate::source::ExternalSource;
use crate::source::GetTargetJob;
use crate::source::OngoingCiRunSource;
use crate::source::ReleaseSource;
use crate::source::Source;
use anyhow::Context;
use clap::Parser;
use derivative::Derivative;
use futures_util::future::try_join;
use ide_ci::actions::workflow::is_in_env;
use ide_ci::cache::Cache;
use ide_ci::global;
use ide_ci::log::setup_logging;
use ide_ci::models::config::RepoContext;
use ide_ci::programs::cargo;
use ide_ci::programs::Cargo;
use ide_ci::programs::Git;
use std::any::type_name;
use std::time::Duration;
use tempfile::tempdir;
use tokio::process::Child;
use tokio::runtime::Runtime;

/// The basic, common information available in this application.
#[derive(Clone, Derivative)]
#[derivative(Debug)]
pub struct BuildContext {
    /// GitHub API client.
    ///
    /// If authorized, it will count API rate limits against our identity and allow operations like
    /// managing releases or downloading CI run artifacts.
    #[derivative(Debug = "ignore")]
    pub octocrab: Octocrab,

    /// Version to be built.
    ///
    /// Note that this affects only targets that are being built. If project parts are provided by
    /// other means, their version might be different.
    pub triple: TargetTriple,

    /// Directory being an `enso` repository's working copy.
    ///
    /// The directory is not required to be a git repository. It is allowed to use source tarballs
    /// as well.
    pub source_root: PathBuf,

    /// Remote repository is used for release-related operations. This also includes deducing a new
    /// version number.
    pub remote_repo: RepoContext,

    /// Stores things like downloaded release assets to save time.
    pub cache: Cache,
}

impl BuildContext {
    /// Setup common build environment information based on command line input and local
    /// environment.
    pub async fn new(cli: &Cli) -> Result<Self> {
        let absolute_repo_path = cli.repo_path.absolutize()?;
        let octocrab = setup_octocrab()?;
        let versions = crate::version::deduce_versions(
            &octocrab,
            BuildKind::Dev,
            Ok(&cli.repo_remote),
            &absolute_repo_path,
        )
        .await?;
        let mut triple = TargetTriple::new(versions);
        triple.os = cli.target_os;
        triple.versions.publish()?;
        Ok(Self {
            octocrab,
            triple,
            source_root: absolute_repo_path.into(),
            remote_repo: cli.repo_remote.clone(),
            cache: Cache::new(&cli.cache_path).await?,
        })
    }

    pub fn resolve<T: IsTargetSource + IsTarget>(
        &self,
        target: T,
        source: arg::Source<T>,
    ) -> BoxFuture<'static, Result<GetTargetJob<T>>>
    where
        T: Resolvable,
    {
        let span = info_span!("Resolving.", ?target, ?source).entered();
        let destination = source.output_path.output_path;
        let source = match source.source {
            arg::SourceKind::Build => {
                let resolved = T::resolve(self, source.build_args);
                ready(resolved.map(Source::BuildLocally)).boxed()
            }
            arg::SourceKind::Local =>
                ready(Ok(Source::External(ExternalSource::LocalFile(source.path.clone())))).boxed(),
            arg::SourceKind::CiRun => {
                let run_id = source.run_id.context(format!(
                    "Missing run ID, please provide {} argument.",
                    T::RUN_ID_NAME
                ));
                ready(run_id.map(|run_id| {
                    Source::External(ExternalSource::CiRun(CiRunSource {
                        octocrab: self.octocrab.clone(),
                        run_id,
                        repository: self.remote_repo.clone(),
                        artifact_name: source.artifact_name.clone(),
                    }))
                }))
                .boxed()
            }
            arg::SourceKind::CurrentCiRun =>
                ready(Ok(Source::External(ExternalSource::OngoingCiRun(OngoingCiRunSource {
                    artifact_name: source.artifact_name.clone(),
                }))))
                .boxed(),
            arg::SourceKind::Release => {
                let designator = source
                    .release
                    .context(format!("Missing {} argument.", T::RELEASE_DESIGNATOR_NAME));
                let resolved = designator
                    .map(|designator| self.resolve_release_designator(target, designator));
                async move { Ok(Source::External(ExternalSource::Release(resolved?.await?))) }
                    .boxed()
            }
        };
        async move { Ok(GetTargetJob { source: source.await?, destination }) }
            .instrument(span.clone())
            .boxed()
    }

    #[tracing::instrument]
    pub fn resolve_release_designator<T: IsTarget>(
        &self,
        target: T,
        designator: String,
    ) -> BoxFuture<'static, Result<ReleaseSource>> {
        let repository = self.remote_repo.clone();
        let octocrab = self.octocrab.clone();
        let designator_cp = designator.clone();
        async move {
            let release = match designator.as_str() {
                "latest" => repository.latest_release(&octocrab).await?,
                "nightly" => crate::version::latest_nightly_release(&octocrab, &repository).await?,
                tag => repository.find_release_by_text(&octocrab, tag).await?,
            };
            Ok(ReleaseSource {
                octocrab,
                repository,
                asset_id: target
                    .find_asset(release.assets)
                    .context(format!(
                        "Failed to find a relevant asset in the release '{}'.",
                        release.tag_name
                    ))?
                    .id,
            })
        }
        .map_err(move |e: anyhow::Error| {
            e.context(format!("Failed to resolve release designator `{designator_cp}`."))
        })
        .boxed()
    }

    pub fn commit(&self) -> BoxFuture<'static, Result<String>> {
        let root = self.source_root.clone();
        async move {
            match ide_ci::actions::env::GITHUB_SHA.get() {
                Ok(commit) => Ok(commit),
                Err(_e) => Git::new(root).head_hash().await,
            }
        }
        .boxed()
    }

    pub fn js_build_info(&self) -> BoxFuture<'static, Result<gui::BuildInfo>> {
        let triple = self.triple.clone();
        let commit = self.commit();
        async move {
            Ok(gui::BuildInfo {
                commit:         commit.await?,
                name:           "Enso IDE".into(),
                version:        triple.versions.version.clone(),
                engine_version: triple.versions.version.clone(),
            })
        }
        .boxed()
    }

    pub fn pm_info(&self) -> crate::project::project_manager::BuildInput {
        crate::project::project_manager::BuildInput {
            octocrab:  self.octocrab.clone(),
            versions:  self.triple.versions.clone(),
            repo_root: self.source_root.clone(),
        }
    }

    pub fn resolve_inputs<T: Resolvable>(
        &self,
        inputs: <T as IsTargetSource>::BuildInput,
    ) -> Result<<T as IsTarget>::BuildInput> {
        T::resolve(self, inputs)
    }

    pub fn get<Target>(
        &self,
        target_source: arg::Source<Target>,
    ) -> BoxFuture<'static, Result<Target::Artifact>>
    where
        Target: IsTarget + IsTargetSource + Send + Sync + 'static,
        Target: Resolvable,
    {
        let target = self.target();
        let get_task = self.target().map(|target| self.resolve(target, target_source));
        let cache = self.cache.clone();
        async move { get_resolved(target?, cache, get_task?.await?).await }.boxed()
    }

    pub fn repo_root(&self) -> RepoRoot {
        RepoRoot::new(&self.source_root, &self.triple.to_string())
    }

    pub fn handle_wasm(&self, wasm: arg::wasm::Target) -> BoxFuture<'static, Result> {
        match wasm.command {
            arg::wasm::Command::Watch { params, output_path } => {
                let inputs = self.resolve_inputs::<Wasm>(params);
                async move {
                    let mut watcher = Wasm.setup_watcher(inputs?, output_path.output_path).await?;
                    watcher.wait_ok().await
                }
                .boxed()
            }
            arg::wasm::Command::Build { params, output_path } => {
                let inputs = self.resolve_inputs::<Wasm>(params);
                let cache = self.cache.clone();
                async move {
                    let source = crate::source::Source::BuildLocally(inputs?);
                    let job = GetTargetJob { source, destination: output_path.output_path };
                    get_resolved(Wasm, cache, job).await?;
                    Ok(())
                }
                .boxed()
            }
            arg::wasm::Command::Check => Wasm.check().boxed(),
            arg::wasm::Command::Test { no_wasm, no_native } =>
                Wasm.test(self.repo_root().path, !no_wasm, !no_native).boxed(),
            arg::wasm::Command::Get { source } => {
                let target = Wasm;
                let source = self.resolve(target, source);
                let cache = self.cache.clone();
                async move {
                    target.get(source.await?, cache).await?;
                    Ok(())
                }
                .boxed()
            }
        }
    }

    pub fn handle_gui(&self, gui: arg::gui::Target) -> BoxFuture<'static, Result> {
        match gui.command {
            arg::gui::Command::Get { source } => {
                let job = self.get(source);
                job.void_ok().boxed()
            }
            arg::gui::Command::Watch { input } => self.watch_gui(input),
        }
    }

    pub fn watch_gui(&self, input: arg::gui::WatchInput) -> BoxFuture<'static, Result> {
        let wasm_target = Wasm;
        let arg::gui::WatchInput { wasm, output_path } = input;
        let source = self.resolve(wasm_target, wasm);
        let repo_root = self.repo_root();
        let build_info = self.js_build_info();
        let cache = self.cache.clone();
        async move {
            let source = source.await?;
            let mut wasm_watcher = wasm_target.watch(source, cache).await?;
            let input = gui::GuiInputs {
                repo_root,
                build_info,
                wasm: ready(Ok(wasm_watcher.as_ref().clone())).boxed(),
            };
            let mut gui_watcher = Gui.setup_watcher(input, output_path).await?;
            try_join(wasm_watcher.wait_ok(), gui_watcher.wait_ok()).void_ok().await
        }
        .boxed()
    }

    pub fn handle_project_manager(
        &self,
        project_manager: arg::project_manager::Target,
    ) -> BoxFuture<'static, Result> {
        let job = self.get(project_manager.source);
        job.void_ok().boxed()
    }

    pub fn handle_ide(&self, ide: arg::ide::Target) -> BoxFuture<'static, Result> {
        match ide.command {
            arg::ide::Command::Build { params } => {
                let build_job = self.build_ide(params);
                async move {
                    let artifacts = build_job.await?;
                    if is_in_env() {
                        artifacts.upload().await?;
                    }
                    Ok(())
                }
                .boxed()
            }
            arg::ide::Command::Start { params } => {
                let build_job = self.build_ide(params);
                async move {
                    let ide = build_job.await?;
                    Command::new(ide.unpacked.join("Enso")).run_ok().await?;
                    Ok(())
                }
                .boxed()
            }
            arg::ide::Command::Watch { project_manager, gui } => {
                use crate::project::ProcessWrapper;
                let gui_watcher = self.watch_gui(gui);
                let project_manager = self.spawn_project_manager(project_manager, None);

                async move {
                    let mut project_manager = project_manager.await?;
                    gui_watcher.await?;
                    project_manager.stdin.take(); // dropping stdin handle should make PM finish
                    project_manager.wait_ok().await?;
                    Ok(())
                }
                .boxed()
            }
            arg::ide::Command::IntegrationTest {
                external_backend,
                project_manager,
                wasm_pack_options,
                headless,
            } => {
                let custom_root = tempdir();
                let (custom_root, project_manager) = match custom_root {
                    Ok(tempdir) => {
                        let custom_root = Some(tempdir.path().into());
                        (
                            Some(tempdir),
                            Ok(self.spawn_project_manager(project_manager, custom_root)),
                        )
                    }
                    Err(e) => (None, Err(e)),
                };
                let source_root = self.source_root.clone();
                async move {
                    let project_manager =
                        if !external_backend { Some(project_manager?.await?) } else { None };
                    Wasm.integration_test(
                        source_root,
                        project_manager,
                        headless,
                        wasm_pack_options,
                    )
                    .await?;
                    // Custom root must live while the tests are being run.
                    drop(custom_root);
                    Ok(())
                }
                .boxed()
            }
        }
    }

    /// Spawns a Project Manager.
    pub fn spawn_project_manager(
        &self,
        source: arg::Source<ProjectManager>,
        custom_root: Option<PathBuf>,
    ) -> BoxFuture<'static, Result<Child>> {
        let get_task = self.get(source);
        async move {
            let project_manager = get_task.await?;
            let mut command = crate::programs::project_manager::spawn_from(&project_manager.path);
            if let Some(custom_root) = custom_root {
                command.set_env(crate::programs::project_manager::PROJECTS_ROOT, &custom_root)?;
            }
            command.spawn_intercepting()
        }
        .boxed()
    }

    pub fn build_ide(
        &self,
        params: arg::ide::BuildInput,
    ) -> BoxFuture<'static, Result<ide::Artifact>> {
        let arg::ide::BuildInput { gui, project_manager, output_path } = params;
        let input = ide::BuildInput {
            gui:             self.get(gui),
            project_manager: self.get(project_manager),
            repo_root:       self.repo_root(),
            version:         self.triple.versions.version.clone(),
        };
        let target = Ide { target_os: self.triple.os };
        target.build(input, output_path)
    }

    pub fn target<Target: Resolvable>(&self) -> Result<Target> {
        Target::prepare_target(self)
    }
}

pub trait Resolvable: IsTarget + IsTargetSource + Clone {
    fn prepare_target(context: &BuildContext) -> Result<Self>;

    fn resolve(
        ctx: &BuildContext,
        from: <Self as IsTargetSource>::BuildInput,
    ) -> Result<<Self as IsTarget>::BuildInput>;
}

impl Resolvable for Wasm {
    fn prepare_target(_context: &BuildContext) -> Result<Self> {
        Ok(Wasm {})
    }

    fn resolve(
        ctx: &BuildContext,
        from: <Self as IsTargetSource>::BuildInput,
    ) -> Result<<Self as IsTarget>::BuildInput> {
        let arg::wasm::BuildInputs {
            crate_path,
            wasm_profile,
            cargo_options,
            profiling_level,
            wasm_size_limit,
        } = from;
        Ok(wasm::BuildInput {
            repo_root: ctx.repo_root(),
            crate_path,
            extra_cargo_options: cargo_options,
            profile: wasm_profile.into(),
            profiling_level: profiling_level.map(into),
            wasm_size_limit,
        })
    }
}

impl Resolvable for Gui {
    fn prepare_target(_context: &BuildContext) -> Result<Self> {
        Ok(Gui {})
    }

    fn resolve(
        ctx: &BuildContext,
        from: <Self as IsTargetSource>::BuildInput,
    ) -> Result<<Self as IsTarget>::BuildInput> {
        Ok(gui::GuiInputs {
            wasm:       ctx.get(from.wasm),
            repo_root:  ctx.repo_root(),
            build_info: ctx.js_build_info(),
        })
    }
}

impl Resolvable for ProjectManager {
    fn prepare_target(context: &BuildContext) -> Result<Self> {
        Ok(ProjectManager { target_os: context.triple.os })
    }

    fn resolve(
        ctx: &BuildContext,
        _from: <Self as IsTargetSource>::BuildInput,
    ) -> Result<<Self as IsTarget>::BuildInput> {
        Ok(project_manager::BuildInput {
            repo_root: ctx.repo_root().path,
            octocrab:  ctx.octocrab.clone(),
            versions:  ctx.triple.versions.clone(),
        })
    }
}

#[tracing::instrument(skip_all, fields(?target, ?get_task))]
pub async fn get_resolved<Target>(
    target: Target,
    cache: Cache,
    get_task: GetTargetJob<Target>,
) -> Result<Target::Artifact>
where
    Target: IsTarget + Send + Sync + 'static,
{
    // We upload only built artifacts. There would be no point in uploading something that
    // we've just downloaded.
    let should_upload_artifact = matches!(get_task.source, Source::BuildLocally(_)) && is_in_env();
    let artifact = target.get(get_task, cache).await?;
    info!(
        "Got target {}, should it be uploaded? {}",
        type_name::<Target>(),
        should_upload_artifact
    );
    if should_upload_artifact {
        let upload_job = target.upload_artifact(ready(Ok(artifact.clone())));
        // global::spawn(upload_job);
        // info!("Spawned upload job for {}.", type_name::<Target>());
        warn!("Forcing the job.");
        upload_job.await?;
    }
    Ok(artifact)
}

#[tracing::instrument(err)]
pub async fn main_internal() -> Result {
    let cli = Cli::parse();
    setup_logging()?;

    pretty_env_logger::init();
    debug!("Parsed CLI arguments: {cli:#?}");

    let ctx = BuildContext::new(&cli).instrument(info_span!("Building context.")).await?;
    match cli.target {
        Target::Wasm(wasm) => ctx.handle_wasm(wasm).await?,
        Target::Gui(gui) => ctx.handle_gui(gui).await?,
        Target::ProjectManager(project_manager) =>
            ctx.handle_project_manager(project_manager).await?,
        Target::Ide(ide) => ctx.handle_ide(ide).await?,
        // TODO: consider if out-of-source ./dist should be removed
        Target::Clean => Git::new(ctx.repo_root()).cmd()?.nice_clean().run_ok().await?,
        Target::Lint => {
            Cargo
                .cmd()?
                .current_dir(ctx.repo_root())
                .arg("clippy")
                .apply(&cargo::Options::Workspace)
                .apply(&cargo::Options::Package("enso-integration-test".into()))
                .apply(&cargo::Options::AllTargets)
                .args(["--", "-D", "warnings"])
                .run_ok()
                .await?;

            Cargo
                .cmd()?
                .current_dir(ctx.repo_root())
                .arg("fmt")
                .args(["--", "--check"])
                .run_ok()
                .await?;

            prettier::check(&ctx.repo_root()).await?;
        }
        Target::Fmt => {
            prettier::write(&ctx.repo_root()).await?;
            Cargo.cmd()?.current_dir(ctx.repo_root()).arg("fmt").run_ok().await?;
        }
    };
    info!("Completed main job.");
    global::complete_tasks().await?;
    Ok(())
}

pub fn main() -> Result {
    let rt = Runtime::new()?;
    rt.block_on(async { main_internal().await })?;
    rt.shutdown_timeout(Duration::from_secs(60 * 30));
    info!("Successfully ending.");
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::version::Versions;

    #[tokio::test]
    async fn resolving_release() -> Result {
        setup_logging()?;
        let octocrab = Octocrab::default();
        let context = BuildContext {
            remote_repo: RepoContext::from_str("enso-org/enso")?,
            triple: TargetTriple::new(Versions::new(Version::new(2022, 1, 1))),
            source_root: r"H:/NBO/enso5".into(),
            octocrab,
            cache: Cache::new_default().await?,
        };

        dbg!(
            context
                .resolve_release_designator(
                    ProjectManager { target_os: TARGET_OS },
                    "latest".into()
                )
                .await
        )?;

        Ok(())
    }
}
