#![feature(explicit_generic_args_with_impl_trait)]
#![feature(option_result_contains)]
#![feature(once_cell)]
#![feature(default_free_fn)]

pub mod arg;
pub mod ci_gen;

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


use crate::arg::java_gen;
use crate::arg::release::Action;
use crate::arg::BuildJob;
use crate::arg::Cli;
use crate::arg::IsTargetSource;
use crate::arg::IsWatchableSource;
use crate::arg::Target;
use crate::arg::WatchJob;
use anyhow::Context;
use clap::Parser;
use derivative::Derivative;
use enso_build::context::BuildContext;
use enso_build::engine::Benchmarks;
use enso_build::engine::BuildMode;
use enso_build::engine::Tests;
use enso_build::paths::TargetTriple;
use enso_build::prettier;
use enso_build::project;
use enso_build::project::backend;
use enso_build::project::backend::Backend;
// use enso_build::project::engine;
// use enso_build::project::engine::Engine;
use enso_build::project::gui;
use enso_build::project::gui::Gui;
use enso_build::project::ide;
use enso_build::project::ide::Ide;
// use enso_build::project::project_manager;
// use enso_build::project::project_manager::ProjectManager;
use enso_build::project::wasm;
use enso_build::project::wasm::Wasm;
use enso_build::project::IsTarget;
use enso_build::project::IsWatchable;
use enso_build::project::IsWatcher;
use enso_build::project::ProcessWrapper;
use enso_build::setup_octocrab;
use enso_build::source::BuildTargetJob;
use enso_build::source::CiRunSource;
use enso_build::source::ExternalSource;
use enso_build::source::GetTargetJob;
use enso_build::source::OngoingCiRunSource;
use enso_build::source::ReleaseSource;
use enso_build::source::Source;
use enso_build::source::WatchTargetJob;
use enso_build::source::WithDestination;
use futures_util::future::try_join;
use ide_ci::actions::workflow::is_in_env;
use ide_ci::cache::Cache;
use ide_ci::fs::remove_if_exists;
use ide_ci::github::release::upload_asset;
use ide_ci::global;
use ide_ci::log::setup_logging;
use ide_ci::ok_ready_boxed;
use ide_ci::programs::cargo;
use ide_ci::programs::git::clean;
use ide_ci::programs::rustc;
use ide_ci::programs::Cargo;
use std::time::Duration;
use tempfile::tempdir;
use tokio::process::Child;
use tokio::runtime::Runtime;

pub fn void<T>(_t: T) {}

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
        // let build_kind = match &cli.target {
        //     Target::Release(release) => release.kind,
        //     _ => enso_build::version::BuildKind::Dev,
        // };
        let absolute_repo_path = cli.repo_path.absolutize()?;
        let octocrab = setup_octocrab().await?;
        let versions = enso_build::version::deduce_versions(
            &octocrab,
            cli.build_kind,
            Ok(&cli.repo_remote),
            &absolute_repo_path,
        )
        .await?;
        let mut triple = TargetTriple::new(versions);
        triple.os = cli.target_os;
        triple.versions.publish()?;
        let context = BuildContext {
            inner: project::Context {
                cache: Cache::new(&cli.cache_path).await?,
                octocrab,
                upload_artifacts: cli.upload_artifacts,
            },
            triple,
            source_root: absolute_repo_path.into(),
            remote_repo: cli.repo_remote.clone(),
        };
        Ok(Self { context })
    }

    pub fn context(&self) -> project::Context {
        self.inner.clone()
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
            arg::SourceKind::Build =>
                T::resolve(self, source.build_args).map_ok(Source::BuildLocally).boxed(),
            arg::SourceKind::Local =>
                ok_ready_boxed(Source::External(ExternalSource::LocalFile(source.path.clone()))),
            arg::SourceKind::CiRun => {
                let run_id = source.run_id.context(format!(
                    "Missing run ID, please provide {} argument.",
                    T::RUN_ID_NAME
                ));
                let source = run_id.map(|run_id| {
                    Source::External(ExternalSource::CiRun(CiRunSource {
                        run_id,
                        repository: self.remote_repo.clone(),
                        artifact_name: resolve_artifact_name(source.artifact_name.clone(), &target),
                    }))
                });
                ready(source).boxed()
            }
            arg::SourceKind::CurrentCiRun =>
                ok_ready_boxed(Source::External(ExternalSource::OngoingCiRun(OngoingCiRunSource {
                    artifact_name: resolve_artifact_name(source.artifact_name.clone(), &target),
                }))),
            arg::SourceKind::Release => {
                let designator = source
                    .release
                    .context(format!("Missing {} argument.", T::RELEASE_DESIGNATOR_NAME));
                let resolved = designator
                    .and_then_async(|designator| self.resolve_release_source(target, designator));
                resolved.map_ok(|source| Source::External(ExternalSource::Release(source))).boxed()
            }
        };
        async move { Ok(GetTargetJob { inner: source.await?, destination }) }
            .instrument(span.clone())
            .boxed()
    }

    #[tracing::instrument]
    pub fn resolve_release_source<T: IsTarget>(
        &self,
        target: T,
        designator: String,
    ) -> BoxFuture<'static, Result<ReleaseSource>> {
        let repository = self.remote_repo.clone();
        let release = self.resolve_release_designator(designator);
        release
            .and_then_sync(move |release| {
                Ok(ReleaseSource {
                    repository,
                    asset_id: target
                        .find_asset(release.assets)
                        .context(format!(
                            "Failed to find a relevant asset in the release '{}'.",
                            release.tag_name
                        ))?
                        .id,
                })
            })
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
            versions:  self.triple.versions.clone(),
            repo_root: self.source_root.clone(),
        }
    }

    pub fn resolve_inputs<T: Resolvable>(
        &self,
        inputs: <T as IsTargetSource>::BuildInput,
    ) -> BoxFuture<'static, Result<<T as IsTarget>::BuildInput>> {
        T::resolve(self, inputs)
    }

    pub fn resolve_watch_inputs<T: WatchResolvable>(
        &self,
        inputs: <T as IsWatchableSource>::WatchInput,
    ) -> Result<<T as IsWatchable>::WatchInput> {
        T::resolve_watch(self, inputs)
    }

    pub fn resolve_build_job<T: Resolvable>(
        &self,
        job: arg::BuildJob<T>,
    ) -> BoxFuture<'static, Result<BuildTargetJob<T>>> {
        let arg::BuildJob { input, output_path } = job;
        let input = self.resolve_inputs::<T>(input);
        async move {
            Ok(WithDestination { destination: output_path.output_path, inner: input.await? })
        }
        .boxed()
    }

    pub fn resolve_watch_job<T: WatchResolvable>(
        &self,
        job: arg::WatchJob<T>,
    ) -> BoxFuture<'static, Result<WatchTargetJob<T>>> {
        let arg::WatchJob { build, watch_input } = job;
        let build = self.resolve_build_job(build);
        let watch_input = self.resolve_watch_inputs::<T>(watch_input);
        async move { Ok(WatchTargetJob { watch_input: watch_input?, build: build.await? }) }.boxed()
    }

    pub fn watch<Target: WatchResolvable>(
        &self,
        job: WatchJob<Target>,
    ) -> BoxFuture<'static, Result<Target::Watcher>> {
        let context = self.context();
        let job = self.resolve_watch_job(job);
        let target = self.target::<Target>();
        async move { target?.watch(context, job.await?).await }.boxed()
    }

    pub fn watch_and_wait<Target: WatchResolvable>(
        &self,
        job: WatchJob<Target>,
    ) -> BoxFuture<'static, Result> {
        let watcher = self.watch(job);
        async move { watcher.await?.wait_for_finish().await }.boxed()
    }

    pub fn get<Target>(
        &self,
        target_source: arg::Source<Target>,
    ) -> BoxFuture<'static, Result<Target::Artifact>>
    where
        Target: IsTarget + IsTargetSource + Send + Sync + 'static,
        Target: Resolvable,
    {
        let target = self.target::<Target>();
        let get_task = self.target().map(|target| self.resolve(target, target_source));
        let context = self.context();
        async move { target?.get(context, get_task?.await?).await }.boxed()
    }

    pub fn build<Target: Resolvable>(&self, job: BuildJob<Target>) -> BoxFuture<'static, Result> {
        let context = self.context();
        let target = self.target::<Target>();
        let job = self.resolve_build_job(job);
        async move {
            let job = job.await?;
            target?.build(context, job).await
        }
        .void_ok()
        .boxed()
    }

    pub fn handle_wasm(&self, wasm: arg::wasm::Target) -> BoxFuture<'static, Result> {
        match wasm.command {
            arg::wasm::Command::Watch(job) => self.watch_and_wait(job),
            arg::wasm::Command::Build(job) => self.build(job).void_ok().boxed(),
            arg::wasm::Command::Check => Wasm.check().boxed(),
            arg::wasm::Command::Test { no_wasm, no_native } =>
                Wasm.test(self.repo_root().path, !no_wasm, !no_native).boxed(),
            arg::wasm::Command::Get(source) => self.get(source).void_ok().boxed(),
        }
    }

    // pub fn handle_engine(&self, engine: arg::engine::Target) -> BoxFuture<'static, Result> {
    //     self.get(engine.source).void_ok().boxed()
    // }
    //
    // pub fn handle_project_manager(
    //     &self,
    //     project_manager: arg::project_manager::Target,
    // ) -> BoxFuture<'static, Result> {
    //     self.get(project_manager.source).void_ok().boxed()
    // }

    pub fn handle_gui(&self, gui: arg::gui::Target) -> BoxFuture<'static, Result> {
        match gui.command {
            arg::gui::Command::Build(job) => self.build(job),
            arg::gui::Command::Get(source) => self.get(source).void_ok().boxed(),
            arg::gui::Command::Watch(job) => self.watch_and_wait(job),
        }
    }

    pub fn handle_backend(&self, backend: arg::backend::Target) -> BoxFuture<'static, Result> {
        match backend.command {
            arg::backend::Command::Get { source } => self.get(source).void_ok().boxed(),
            arg::backend::Command::Upload { input } => {
                let input = enso_build::project::Backend::resolve(self, input);
                let repo = self.remote_repo.clone();
                let context = self.context();
                async move {
                    let input = input.await?;
                    let operation = enso_build::engine::Operation::Release(
                        enso_build::engine::ReleaseOperation {
                            repo,
                            command: enso_build::engine::ReleaseCommand::Upload,
                        },
                    );
                    let config = enso_build::engine::BuildConfigurationFlags {
                        mode: BuildMode::NightlyRelease,
                        build_engine_package: true,
                        build_launcher_bundle: true,
                        build_project_manager_bundle: true,
                        ..default()
                    };
                    let context = input.prepare_context(context, operation, config)?;
                    context.execute().await?;
                    Ok(())
                }
                .boxed()
            }
            arg::backend::Command::Benchmark { which, minimal_run } => {
                let config = enso_build::engine::BuildConfigurationFlags {
                    execute_benchmarks: which.into_iter().collect(),
                    execute_benchmarks_once: minimal_run,
                    ..default()
                };
                let context = self.prepare_backend_context(config);
                async move {
                    let context = context.await?;
                    context.execute().await
                }
                .boxed()
            }
            arg::backend::Command::Test { which } => {
                let mut config = enso_build::engine::BuildConfigurationFlags::default();
                for arg in which {
                    match arg {
                        Tests::Scala => config.test_scala = true,
                        Tests::StandardLibrary => config.test_standard_library = true,
                    }
                }
                config.test_java_generated_from_rust = true;
                let context = self.prepare_backend_context(config);
                async move { context.await?.execute().await }.boxed()
            }
            arg::backend::Command::Sbt { command } => {
                let context = self.prepare_backend_context(default());
                async move {
                    let mut command_pieces = vec![OsString::from("sbt")];
                    command_pieces.extend(command.into_iter().map(into));

                    let mut context = context.await?;
                    context.operation =
                        enso_build::engine::Operation::Run(enso_build::engine::RunOperation {
                            command_pieces,
                        });
                    context.execute().await
                }
                .boxed()
            }
            arg::backend::Command::CiCheck {} => {
                let config = enso_build::engine::BuildConfigurationFlags {
                    mode: BuildMode::Development,
                    test_scala: true,
                    test_standard_library: true,
                    test_java_generated_from_rust: true,
                    build_benchmarks: true,
                    execute_benchmarks: once(Benchmarks::Runtime).collect(),
                    execute_benchmarks_once: true,
                    build_js_parser: matches!(TARGET_OS, OS::Linux),
                    ..default()
                };
                let context = self.prepare_backend_context(config);
                async move {
                    let mut context = context.await?;
                    context.upload_artifacts = true;
                    context.execute().await
                }
                .boxed()
            }
        }
    }

    #[instrument]
    pub fn prepare_backend_context(
        &self,
        config: enso_build::engine::BuildConfigurationFlags,
    ) -> BoxFuture<'static, Result<enso_build::engine::RunContext>> {
        let operation = enso_build::engine::Operation::Build;
        let paths = enso_build::paths::Paths::new_triple(&self.source_root, self.triple.clone());
        let config = config.into();
        let octocrab = self.octocrab.clone();
        async move {
            let paths = paths?;
            let goodies = ide_ci::goodie::GoodieDatabase::new()?;
            let inner = crate::project::Context {
                upload_artifacts: true,
                octocrab,
                cache: Cache::new_default().await?,
            };
            Ok(enso_build::engine::RunContext { inner, config, paths, goodies, operation })
        }
        .boxed()
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
            arg::ide::Command::Start { params, ide_option } => {
                let build_job = self.build_ide(params);
                async move {
                    let ide = build_job.await?;
                    ide.start_unpacked(ide_option).run_ok().await?;
                    Ok(())
                }
                .boxed()
            }
            arg::ide::Command::Watch { project_manager, gui } => {
                let gui_watcher = self.watch(gui);
                let project_manager = self.spawn_project_manager(project_manager, None);

                async move {
                    let mut project_manager = project_manager.await?;
                    let mut gui_watcher = gui_watcher.await?;
                    gui_watcher.wait_for_finish().await?;
                    debug!("GUI watcher has finished, ending Project Manager process.");
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
    ) -> BoxFuture<'static, Result<<Self as IsTarget>::BuildInput>>;
}

impl Resolvable for Wasm {
    fn prepare_target(_context: &Processor) -> Result<Self> {
        Ok(Wasm {})
    }

    fn resolve(
        ctx: &Processor,
        from: <Self as IsTargetSource>::BuildInput,
    ) -> BoxFuture<'static, Result<<Self as IsTarget>::BuildInput>> {
        let arg::wasm::BuildInput {
            crate_path,
            wasm_profile,
            wasm_opt_option: wasm_opt_options,
            cargo_options,
            profiling_level,
            wasm_size_limit,
            skip_wasm_opt,
        } = from;
        ok_ready_boxed(wasm::BuildInput {
            repo_root: ctx.repo_root(),
            crate_path,
            wasm_opt_options,
            skip_wasm_opt,
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
    ) -> BoxFuture<'static, Result<<Self as IsTarget>::BuildInput>> {
        let wasm_source = ctx.resolve(Wasm, from.wasm);
        let repo_root = ctx.repo_root();
        let build_info = ctx.js_build_info();
        async move { Ok(gui::BuildInput { wasm: wasm_source.await?, repo_root, build_info }) }
            .boxed()
    }
}

impl Resolvable for Backend {
    fn prepare_target(context: &Processor) -> Result<Self> {
        Ok(Backend { target_os: context.triple.os })
    }

    fn resolve(
        ctx: &Processor,
        _from: <Self as IsTargetSource>::BuildInput,
    ) -> BoxFuture<'static, Result<<Self as IsTarget>::BuildInput>> {
        ok_ready_boxed(backend::BuildInput {
            repo_root: ctx.repo_root().path,
            versions:  ctx.triple.versions.clone(),
        })
    }
}

// impl Resolvable for ProjectManager {
//     fn prepare_target(_context: &Processor) -> Result<Self> {
//         Ok(ProjectManager)
//     }
//
//     fn resolve(
//         ctx: &Processor,
//         _from: <Self as IsTargetSource>::BuildInput,
//     ) -> BoxFuture<'static, Result<<Self as IsTarget>::BuildInput>> {
//         ok_ready_boxed(project_manager::BuildInput {
//             repo_root: ctx.repo_root().path,
//             versions:  ctx.triple.versions.clone(),
//         })
//     }
// }
//
// impl Resolvable for Engine {
//     fn prepare_target(_context: &Processor) -> Result<Self> {
//         Ok(Engine)
//     }
//
//     fn resolve(
//         ctx: &Processor,
//         _from: <Self as IsTargetSource>::BuildInput,
//     ) -> BoxFuture<'static, Result<<Self as IsTarget>::BuildInput>> {
//         ok_ready_boxed(engine::BuildInput {
//             repo_root: ctx.repo_root().path,
//             versions:  ctx.triple.versions.clone(),
//         })
//     }
// }

pub trait WatchResolvable: Resolvable + IsWatchableSource + IsWatchable {
    fn resolve_watch(
        ctx: &Processor,
        from: <Self as IsWatchableSource>::WatchInput,
    ) -> Result<<Self as IsWatchable>::WatchInput>;
}

impl WatchResolvable for Wasm {
    fn resolve_watch(
        _ctx: &Processor,
        from: <Self as IsWatchableSource>::WatchInput,
    ) -> Result<<Self as IsWatchable>::WatchInput> {
        Ok(wasm::WatchInput { cargo_watch_options: from.cargo_watch_option })
    }
}

impl WatchResolvable for Gui {
    fn resolve_watch(
        ctx: &Processor,
        from: <Self as IsWatchableSource>::WatchInput,
    ) -> Result<<Self as IsWatchable>::WatchInput> {
        Ok(gui::WatchInput { wasm: Wasm::resolve_watch(ctx, from.wasm)?, shell: from.gui_shell })
    }
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

    // TRANSITION: Previous Engine CI job used to clone these both repositories side-by-side.
    // This collides with GraalVM native image build location.
    if is_in_env() {
        remove_if_exists(cli.repo_path.join("enso"))?;
        remove_if_exists(cli.repo_path.join("ci-build"))?;
    }

    let ctx: Processor = Processor::new(&cli).instrument(info_span!("Building context.")).await?;
    match cli.target {
        Target::Wasm(wasm) => ctx.handle_wasm(wasm).await?,
        Target::Gui(gui) => ctx.handle_gui(gui).await?,
        // Target::ProjectManager(project_manager) =>
        //     ctx.handle_project_manager(project_manager).await?,
        // Target::Engine(engine) => ctx.handle_engine(engine).await?,
        Target::Backend(backend) => ctx.handle_backend(backend).await?,
        Target::Ide(ide) => ctx.handle_ide(ide).await?,
        // TODO: consider if out-of-source ./dist should be removed
        Target::GitClean(options) => {
            let mut exclusions = vec![".idea"];
            if !options.build_script {
                exclusions.push("target/enso-build");
            }

            let git_clean = clean::clean_except_for(ctx.repo_root(), exclusions);
            let clean_cache = async {
                if options.cache {
                    ide_ci::fs::tokio::remove_dir_if_exists(ctx.cache.path()).await?;
                }
                Result::Ok(())
            };
            try_join(git_clean, clean_cache).await?;
        }
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
            Action::DeployToEcr(args) => {
                enso_build::release::deploy_to_ecr(&*ctx, args.ecr_repository).await?;
            }
            Action::Publish => {
                enso_build::release::publish_release(&*ctx).await?;
            }
        },
        Target::CiGen => ci_gen::generate(
            &enso_build::paths::generated::RepoRootGithubWorkflows::new(cli.repo_path),
        )?,
        Target::JavaGen(command) => {
            let repo_root = ctx.repo_root();
            async move {
                let generate_job = enso_build::rust::parser::generate_java(&repo_root);
                match command.action {
                    java_gen::Command::Build => generate_job.await,
                    java_gen::Command::Test => {
                        generate_job.await?;
                        let backend_context = ctx.prepare_backend_context(default()).await?;
                        backend_context.prepare_build_env().await?;
                        enso_build::rust::parser::run_self_tests(&repo_root).await
                    }
                }
            }
            .await?;
        }
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


// #[cfg(test)]
// mod tests {
//     use super::*;
//     use enso_build::version::Versions;
//     use ide_ci::models::config::RepoContext;
//
//     #[tokio::test]
//     async fn resolving_release() -> Result {
//         setup_logging()?;
//         let octocrab = Octocrab::default();
//         let context = Processor {
//             context: BuildContext {
//                 remote_repo: RepoContext::from_str("enso-org/enso")?,
//                 triple: TargetTriple::new(Versions::new(Version::new(2022, 1, 1))),
//                 source_root: r"H:/NBO/enso5".into(),
//                 octocrab,
//                 cache: Cache::new_default().await?,
//             },
//         };
//
//         dbg!(
//             context.resolve_release_source(Backend { target_os: TARGET_OS },
//     "latest".into()).await     )?;
//
//         Ok(())
//     }
// }
