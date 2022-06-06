#![feature(option_result_contains)]
#![feature(once_cell)]
#![feature(default_free_fn)]

pub mod arg;
pub mod args;

pub mod prelude {
    pub use crate::arg::ArgExt as _;
    pub use enso_build::prelude::*;
}

use crate::prelude::*;

use ide_ci::env::Variable;

pub struct BuildKind;
impl Variable for BuildKind {
    const NAME: &'static str = "ENSO_BUILD_KIND";
    type Value = enso_build::version::BuildKind;
}

// #![feature(explicit_generic_args_with_impl_trait)]
// #![feature(once_cell)]
// #![feature(exit_status_error)]
// #![feature(associated_type_defaults)]
// #![feature(is_some_with)]
// #![feature(adt_const_params)]


use crate::arg::release::Action;
use crate::arg::Cli;
use crate::arg::IsTargetSource;
use crate::arg::OutputPath;
use crate::arg::Target;
use anyhow::Context;
use clap::Parser;
use derivative::Derivative;
use enso_build::context::BuildContext;
use enso_build::paths::TargetTriple;
use enso_build::prettier;
use enso_build::project::backend;
use enso_build::project::backend::Backend;
use enso_build::project::engine;
use enso_build::project::engine::Engine;
use enso_build::project::gui;
use enso_build::project::gui::Gui;
use enso_build::project::ide;
use enso_build::project::ide::Ide;
use enso_build::project::project_manager;
use enso_build::project::project_manager::ProjectManager;
use enso_build::project::wasm;
use enso_build::project::wasm::Wasm;
use enso_build::project::IsTarget;
use enso_build::project::IsWatchable;
use enso_build::project::IsWatcher;
use enso_build::setup_octocrab;
use enso_build::source::CiRunSource;
use enso_build::source::ExternalSource;
use enso_build::source::GetTargetJob;
use enso_build::source::OngoingCiRunSource;
use enso_build::source::ReleaseSource;
use enso_build::source::Source;
use futures_util::future::try_join;
use ide_ci::actions::workflow::is_in_env;
use ide_ci::cache::Cache;
use ide_ci::github::release::upload_asset;
use ide_ci::global;
use ide_ci::log::setup_logging;
use ide_ci::programs::cargo;
use ide_ci::programs::rustc;
use ide_ci::programs::Cargo;
use ide_ci::programs::Git;
use std::any::type_name;
use std::time::Duration;
use tempfile::tempdir;
use tokio::process::Child;
use tokio::runtime::Runtime;

fn resolve_artifact_name(input: Option<String>, project: &impl IsTarget) -> String {
    input.unwrap_or_else(|| project.artifact_name())
}

/// The basic, common information available in this application.
#[derive(Clone, Derivative)]
#[derivative(Debug)]
pub struct Processor {
    pub context: BuildContext,
}

impl Deref for Processor {
    type Target = BuildContext;

    fn deref(&self) -> &Self::Target {
        &self.context
    }
}

impl Processor {
    /// Setup common build environment information based on command line input and local
    /// environment.
    pub async fn new(cli: &Cli) -> Result<Self> {
        let build_kind = match &cli.target {
            Target::Release(release) => release.kind,
            _ => enso_build::version::BuildKind::Dev,
        };
        let absolute_repo_path = cli.repo_path.absolutize()?;
        let octocrab = setup_octocrab().await?;
        let versions = enso_build::version::deduce_versions(
            &octocrab,
            build_kind,
            Ok(&cli.repo_remote),
            &absolute_repo_path,
        )
        .await?;
        let mut triple = TargetTriple::new(versions);
        triple.os = cli.target_os;
        triple.versions.publish()?;
        let context = BuildContext {
            octocrab,
            triple,
            source_root: absolute_repo_path.into(),
            remote_repo: cli.repo_remote.clone(),
            cache: Cache::new(&cli.cache_path).await?,
        };
        Ok(Self { context })
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
                        artifact_name: resolve_artifact_name(source.artifact_name.clone(), &target),
                    }))
                }))
                .boxed()
            }
            arg::SourceKind::CurrentCiRun =>
                ready(Ok(Source::External(ExternalSource::OngoingCiRun(OngoingCiRunSource {
                    artifact_name: resolve_artifact_name(source.artifact_name.clone(), &target),
                }))))
                .boxed(),
            arg::SourceKind::Release => {
                let designator = source
                    .release
                    .context(format!("Missing {} argument.", T::RELEASE_DESIGNATOR_NAME));
                let resolved =
                    designator.map(|designator| self.resolve_release_source(target, designator));
                async move { Ok(Source::External(ExternalSource::Release(resolved?.await?))) }
                    .boxed()
            }
        };
        async move { Ok(GetTargetJob { source: source.await?, destination }) }
            .instrument(span.clone())
            .boxed()
    }

    #[tracing::instrument]
    pub fn resolve_release_source<T: IsTarget>(
        &self,
        target: T,
        designator: String,
    ) -> BoxFuture<'static, Result<ReleaseSource>> {
        let release = self.deref().resolve_release_designator(designator);
        let repository = self.remote_repo.clone();
        let octocrab = self.octocrab.clone();
        async move {
            let release = release.await?;
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

    pub fn pm_info(&self) -> enso_build::project::backend::BuildInput {
        enso_build::project::backend::BuildInput {
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

    pub fn build_locally<Target: Resolvable>(
        &self,
        input: <Target as IsTargetSource>::BuildInput,
        output_path: OutputPath<Target>,
    ) -> BoxFuture<'static, Result> {
        let job = self.resolve_inputs::<Target>(input).and_then(|input| {
            self.target().map(|target: Target| target.build_locally(input, output_path))
        });
        async move { job?.await }.void_ok().boxed()
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
                    let source = enso_build::source::Source::BuildLocally(inputs?);
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

    pub fn handle_engine(&self, engine: arg::engine::Target) -> BoxFuture<'static, Result> {
        self.get(engine.source).void_ok().boxed()
    }

    pub fn handle_project_manager(
        &self,
        project_manager: arg::project_manager::Target,
    ) -> BoxFuture<'static, Result> {
        self.get(project_manager.source).void_ok().boxed()
    }

    pub fn handle_gui(&self, gui: arg::gui::Target) -> BoxFuture<'static, Result> {
        match gui.command {
            arg::gui::Command::Build { input, output_path } =>
                self.build_locally(input, output_path),
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
            let input = gui::BuildInput {
                repo_root,
                build_info,
                wasm: ready(Ok(wasm_watcher.as_ref().clone())).boxed(),
            };
            let mut gui_watcher = Gui.setup_watcher(input, output_path).await?;
            try_join(wasm_watcher.wait_ok(), gui_watcher.wait_ok()).void_ok().await
        }
        .boxed()
    }

    pub fn handle_backend(
        &self,
        project_manager: arg::backend::Target,
    ) -> BoxFuture<'static, Result> {
        let job = self.get(project_manager.source);
        job.void_ok().boxed()
    }

    pub fn handle_ide(&self, ide: arg::ide::Target) -> BoxFuture<'static, Result> {
        match ide.command {
            arg::ide::Command::Build { params } => self.build_ide(params).void_ok().boxed(),
            arg::ide::Command::Upload { params, release_id } => {
                let build_job = self.build_ide(params);
                let remote_repo = self.remote_repo.clone();
                let client = self.octocrab.client.clone();
                async move {
                    let artifacts = build_job.await?;
                    upload_asset(&remote_repo, &client, release_id, &artifacts.image).await?;
                    upload_asset(&remote_repo, &client, release_id, &artifacts.image_checksum)
                        .await?;
                    Ok(())
                }
                .boxed()
            }
            arg::ide::Command::Start { params } => {
                let build_job = self.build_ide(params);
                async move {
                    let ide = build_job.await?;
                    ide.start_unpacked().run_ok().await?;
                    Ok(())
                }
                .boxed()
            }
            arg::ide::Command::Watch { project_manager, gui } => {
                use enso_build::project::ProcessWrapper;
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
                wasm_timeout,
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
                        Some(wasm_timeout.into()),
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
        source: arg::Source<Backend>,
        custom_root: Option<PathBuf>,
    ) -> BoxFuture<'static, Result<Child>> {
        let get_task = self.get(source);
        async move {
            let project_manager = get_task.await?;
            let mut command =
                enso_build::programs::project_manager::spawn_from(&project_manager.path);
            if let Some(custom_root) = custom_root {
                command
                    .set_env(enso_build::programs::project_manager::PROJECTS_ROOT, &custom_root)?;
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
        let target = Ide { target_os: self.triple.os, target_arch: self.triple.arch };
        let build_job = target.build(input, output_path);
        async move {
            let artifacts = build_job.await?;
            if is_in_env() {
                artifacts.upload_as_ci_artifact().await?;
            }
            Ok(artifacts)
        }
        .boxed()
    }

    pub fn target<Target: Resolvable>(&self) -> Result<Target> {
        Target::prepare_target(self)
    }
}

pub trait Resolvable: IsTarget + IsTargetSource + Clone {
    fn prepare_target(context: &Processor) -> Result<Self>;

    fn resolve(
        ctx: &Processor,
        from: <Self as IsTargetSource>::BuildInput,
    ) -> Result<<Self as IsTarget>::BuildInput>;
}

impl Resolvable for Wasm {
    fn prepare_target(_context: &Processor) -> Result<Self> {
        Ok(Wasm {})
    }

    fn resolve(
        ctx: &Processor,
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
            wasm_size_limit: wasm_size_limit.filter(|size_limit| size_limit.get_bytes() > 0),
        })
    }
}

impl Resolvable for Gui {
    fn prepare_target(_context: &Processor) -> Result<Self> {
        Ok(Gui {})
    }

    fn resolve(
        ctx: &Processor,
        from: <Self as IsTargetSource>::BuildInput,
    ) -> Result<<Self as IsTarget>::BuildInput> {
        Ok(gui::BuildInput {
            wasm:       ctx.get(from.wasm),
            repo_root:  ctx.repo_root(),
            build_info: ctx.js_build_info(),
        })
    }
}

impl Resolvable for Backend {
    fn prepare_target(context: &Processor) -> Result<Self> {
        Ok(Backend { target_os: context.triple.os })
    }

    fn resolve(
        ctx: &Processor,
        _from: <Self as IsTargetSource>::BuildInput,
    ) -> Result<<Self as IsTarget>::BuildInput> {
        Ok(backend::BuildInput {
            repo_root: ctx.repo_root().path,
            octocrab:  ctx.octocrab.clone(),
            versions:  ctx.triple.versions.clone(),
        })
    }
}

impl Resolvable for ProjectManager {
    fn prepare_target(_context: &Processor) -> Result<Self> {
        Ok(ProjectManager)
    }

    fn resolve(
        ctx: &Processor,
        _from: <Self as IsTargetSource>::BuildInput,
    ) -> Result<<Self as IsTarget>::BuildInput> {
        Ok(project_manager::BuildInput {
            repo_root: ctx.repo_root().path,
            octocrab:  ctx.octocrab.clone(),
            versions:  ctx.triple.versions.clone(),
        })
    }
}

impl Resolvable for Engine {
    fn prepare_target(_context: &Processor) -> Result<Self> {
        Ok(Engine)
    }

    fn resolve(
        ctx: &Processor,
        _from: <Self as IsTargetSource>::BuildInput,
    ) -> Result<<Self as IsTarget>::BuildInput> {
        Ok(engine::BuildInput {
            repo_root: ctx.repo_root().path,
            octocrab:  ctx.octocrab.clone(),
            versions:  ctx.triple.versions.clone(),
        })
    }
}

#[tracing::instrument(skip_all, fields(?target, ?get_task), err)]
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
pub async fn main_internal(config: enso_build::config::Config) -> Result {
    setup_logging()?;

    // Setup that affects Cli parser construction.
    if let Some(wasm_size_limit) = config.wasm_size_limit {
        crate::arg::wasm::initialize_default_wasm_size_limit(wasm_size_limit)?;
    }

    let cli = Cli::parse();

    debug!("Parsed CLI arguments: {cli:#?}");

    if !cli.skip_version_check {
        config.check_programs().await?;
    }

    let ctx = Processor::new(&cli).instrument(info_span!("Building context.")).await?;
    match cli.target {
        Target::Wasm(wasm) => ctx.handle_wasm(wasm).await?,
        Target::Gui(gui) => ctx.handle_gui(gui).await?,
        Target::ProjectManager(project_manager) =>
            ctx.handle_project_manager(project_manager).await?,
        Target::Engine(engine) => ctx.handle_engine(engine).await?,
        Target::Backend(backend) => ctx.handle_backend(backend).await?,
        Target::Ide(ide) => ctx.handle_ide(ide).await?,
        // TODO: consider if out-of-source ./dist should be removed
        Target::GitClean => Git::new(ctx.repo_root()).cmd()?.nice_clean().run_ok().await?,
        Target::Lint => {
            Cargo
                .cmd()?
                .current_dir(ctx.repo_root())
                .arg(cargo::clippy::COMMAND)
                .apply(&cargo::Options::Workspace)
                .apply(&cargo::Options::Package("enso-integration-test".into()))
                .apply(&cargo::Options::AllTargets)
                .apply(&cargo::Color::Always)
                .arg("--")
                .apply(&rustc::Option::Deny(rustc::Lint::Warnings))
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
        Target::Release(release) => match release.action {
            Action::CreateDraft => {
                enso_build::release::create_release(&*ctx).await?;
            }
            Action::Publish => {
                enso_build::release::publish_release(&*ctx).await?;
            }
        },
    };
    info!("Completed main job.");
    global::complete_tasks().await?;
    Ok(())
}

pub fn lib_main(config: enso_build::config::Config) -> Result {
    let rt = Runtime::new()?;
    rt.block_on(async { main_internal(config).await })?;
    rt.shutdown_timeout(Duration::from_secs(60 * 30));
    info!("Successfully ending.");
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;
    use enso_build::version::Versions;
    use ide_ci::models::config::RepoContext;

    #[tokio::test]
    async fn resolving_release() -> Result {
        setup_logging()?;
        let octocrab = Octocrab::default();
        let context = Processor {
            context: BuildContext {
                remote_repo: RepoContext::from_str("enso-org/enso")?,
                triple: TargetTriple::new(Versions::new(Version::new(2022, 1, 1))),
                source_root: r"H:/NBO/enso5".into(),
                octocrab,
                cache: Cache::new_default().await?,
            },
        };

        dbg!(
            context.resolve_release_source(Backend { target_os: TARGET_OS }, "latest".into()).await
        )?;

        Ok(())
    }
}
