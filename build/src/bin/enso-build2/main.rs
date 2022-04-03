#![feature(explicit_generic_args_with_impl_trait)]
#![feature(once_cell)]

pub mod arg;
pub use enso_build::prelude;

use enso_build::prelude::*;

use crate::arg::Cli;
use crate::arg::GuiCommand;
use crate::arg::GuiTarget;
use crate::arg::IdeCommand;
use crate::arg::IdeTarget;
use crate::arg::IsTargetSource;
use crate::arg::ProjectManagerTarget;
use crate::arg::Target;
use crate::arg::TargetSource;
use crate::arg::TargetSourceArg;
use crate::arg::WasmTarget;
use anyhow::Context;
use clap::Parser;
use enso_build::args::BuildKind;
use enso_build::paths::generated::RepoRoot;
use enso_build::paths::TargetTriple;
use enso_build::project::gui::BuildInfo;
use enso_build::project::gui::Gui;
use enso_build::project::gui::GuiInputs;
use enso_build::project::ide::Ide;
use enso_build::project::project_manager::ProjectManager;
use enso_build::project::wasm;
use enso_build::project::wasm::Wasm;
use enso_build::project::CiRunSource;
use enso_build::project::IsArtifact;
use enso_build::project::IsTarget;
use enso_build::setup_octocrab;
use futures_util::future::try_join;
use ide_ci::actions::workflow::is_in_env;
use ide_ci::global;
use ide_ci::models::config::RepoContext;
use ide_ci::programs::Git;
use std::any::type_name;
use std::time::Duration;
use tokio::runtime::Runtime;

pub enum Source {
    BuildLocally,
    OngoingCiRun,
    CiRun(CiRunSource),
    LocalFile(PathBuf),
}

/// The basic, common information provided by this entry point.
pub struct BuildContext {
    /// GitHub API client.
    ///
    /// If authorized, it will count API rate limits against our identity and allow operations like
    /// managing releases.
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
}

impl BuildContext {
    pub async fn new(cli: &Cli) -> Result<Self> {
        let octocrab = setup_octocrab()?;
        let versions = enso_build::version::deduce_versions(
            &octocrab,
            BuildKind::Dev,
            Some(&cli.repo_remote),
            &cli.repo_path,
        )
        .await?;
        let triple = TargetTriple::new(versions);
        triple.versions.publish()?;
        Ok(Self {
            octocrab,
            triple,
            source_root: cli.repo_path.clone(),
            remote_repo: cli.repo_remote.clone(),
        })
    }

    pub fn resolve<T: IsTargetSource>(&self, source: &TargetSourceArg<T>) -> Result<Source> {
        Ok(match source.source {
            TargetSource::Build => Source::BuildLocally,
            TargetSource::Local => Source::LocalFile(
                source.path.clone().context("Missing path to the local artifacts!")?,
            ),
            TargetSource::CiRun => Source::CiRun(CiRunSource {
                octocrab:      self.octocrab.clone(),
                run_id:        source.run_id.context(format!(
                    "Missing run ID, please provide {} argument.",
                    T::RUN_ID_NAME
                ))?,
                repository:    self.remote_repo.clone(),
                artifact_name: source.artifact_name.clone(),
            }),
            // TargetSource::OngoingCiRun => Source::OngoingCiRun,
            TargetSource::Whatever => {
                todo!()
            }
        })
    }

    pub fn commit(&self) -> BoxFuture<'static, Result<String>> {
        let root = self.source_root.clone();
        async move {
            match ide_ci::actions::env::Sha.fetch() {
                Ok(commit) => Ok(commit),
                Err(_e) => Git::new(root).head_hash().await,
            }
        }
        .boxed()
    }

    pub fn wasm_build_input(&self) -> wasm::BuildInput {
        wasm::BuildInput {
            crate_path: PathBuf::from(wasm::TARGET_CRATE), // FIXME name as arg
            repo_root:  self.repo_root(),
        }
    }

    pub fn js_build_info(&self) -> BoxFuture<'static, Result<BuildInfo>> {
        let triple = self.triple.clone();
        let commit = self.commit();
        async move {
            Ok(BuildInfo {
                commit:         commit.await?,
                name:           "Enso IDE".into(),
                version:        triple.versions.version.clone(),
                engine_version: triple.versions.version.clone(),
            })
        }
        .boxed()
    }

    pub fn pm_info(&self) -> enso_build::project::project_manager::BuildInput {
        enso_build::project::project_manager::BuildInput {
            octocrab:  self.octocrab.clone(),
            versions:  self.triple.versions.clone(),
            repo_root: self.source_root.clone(),
        }
    }

    pub fn get<Target: IsTarget + IsTargetSource + Send + Sync + 'static>(
        &self,
        target: Target,
        target_source: TargetSourceArg<Target>,
        inputs: Target::BuildInput,
    ) -> BoxFuture<'static, Result<Target::Output>> {
        let output_path = target_source.output_path.clone();
        let source = self.resolve(&target_source);
        // We upload only built artifacts. There would be no point in uploading something that we've
        // just downloaded.
        let should_upload_artifact = target_source.source == TargetSource::Build && is_in_env();
        async move {
            info!("Getting target {}.", type_name::<Target>());
            let artifact = get_target(&target, source?, move || Ok(inputs), output_path).await?;
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
        .boxed()
    }

    pub fn repo_root(&self) -> RepoRoot {
        RepoRoot::new(&self.source_root, &self.triple.to_string())
    }

    pub fn get_wasm(&self, source: WasmTarget) -> BoxFuture<'static, Result<wasm::Artifacts>> {
        let input = self.wasm_build_input();
        self.get(Wasm, source.wasm, input)
    }

    pub fn gui_inputs(&self, wasm: TargetSourceArg<Wasm>) -> GuiInputs {
        let wasm = self.get_wasm(WasmTarget { wasm });
        let build_info = self.js_build_info().boxed();
        let repo_root = self.repo_root();
        GuiInputs { wasm, build_info, repo_root }
    }

    pub fn get_gui(&self, source: GuiTarget) -> BoxFuture<'static, Result> {
        match source.command {
            GuiCommand::Build => {
                let inputs = self.gui_inputs(source.wasm.clone());
                let build = self.get(Gui, source.gui, inputs);
                build.map(|_| Ok(())).boxed()
            }
            GuiCommand::Watch => {
                let build_info = self.js_build_info().boxed();
                let repo_root = self.repo_root();
                let watcher = Wasm.watch(self.wasm_build_input(), source.wasm.output_path);
                async move {
                    let wasm::Watcher { mut watch_process, artifacts } = watcher?;
                    let inputs =
                        GuiInputs { wasm: ready(Ok(artifacts)).boxed(), build_info, repo_root };
                    let js_watch = Gui.watch(inputs);
                    try_join(watch_process.wait().map_err(anyhow::Error::from), js_watch)
                        .map_ok(|_| ())
                        .await
                }
                .boxed()
            }
        }
    }

    pub fn get_project_manager(
        &self,
        source: ProjectManagerTarget,
    ) -> BoxFuture<'static, Result<enso_build::project::project_manager::Artifact>> {
        let input = enso_build::project::project_manager::BuildInput {
            repo_root: self.source_root.clone(),
            versions:  self.triple.versions.clone(),
            octocrab:  self.octocrab.clone(),
        };
        self.get(ProjectManager, source.project_manager, input)
    }

    pub fn get_ide(&self, source: IdeTarget) -> BoxFuture<'static, Result> {
        match source.command {
            IdeCommand::Build => {
                let input = enso_build::project::ide::BuildInput {
                    repo_root:       self.repo_root(),
                    version:         self.triple.versions.version.clone(),
                    project_manager: self.get_project_manager(ProjectManagerTarget {
                        project_manager: source.project_manager,
                    }),
                    gui:             self.get(Gui, source.gui, self.gui_inputs(source.wasm)),
                };
                async move {
                    Ide.build(input, source.ide_output_path).await?;
                    Ok(())
                }
                .boxed()
            }
            IdeCommand::Watch => {
                todo!()
            }
        }
        // let input = enso_build::project::ide::BuildInput {
        //     repo_root: self.repo_root(),
        //     version:  self.triple.versions.version.clone(),
        //     project_manager: self.get_project_manager(source.project_manager_source),
        //     gui:             self.gBoxFuture<'static, Result<crate::project::gui::Artifact>>,
        // };
        // self.get(Ide, source.ide_source, input)
    }
}

pub fn get_target<Target: IsTarget>(
    target: &Target,
    source: Source,
    get_inputs: impl FnOnce() -> Result<Target::BuildInput> + Send + 'static,
    output: PathBuf,
) -> BoxFuture<'static, Result<Target::Output>> {
    match source {
        Source::BuildLocally => match get_inputs() {
            Ok(inputs) => target.build(inputs, output),
            Err(e) => ready(Err(e)).boxed(),
        },
        Source::CiRun(ci_run) => target.download_artifact(ci_run, output),
        Source::OngoingCiRun => {
            let artifact_name = target.artifact_name().to_string();
            async move {
                ide_ci::actions::artifacts::download_single_file_artifact(
                    artifact_name,
                    output.as_path(),
                )
                .await?;
                Target::Output::from_existing(output).await
            }
            .boxed()
        }
        Source::LocalFile(source_dir) => async move {
            ide_ci::fs::mirror_directory(source_dir, output.as_path()).await?;
            Target::Output::from_existing(output).await
        }
        .boxed(),
    }
}

async fn main_internal() -> Result {
    // console_subscriber::init();
    pretty_env_logger::init();
    debug!("Setting up.");
    let cli = Cli::try_parse()?;
    trace!("Parsed CLI arguments: {cli:#?}");

    let ctx = BuildContext::new(&cli).await?;

    match cli.target {
        Target::Wasm(wasm) => {
            ctx.get_wasm(wasm).await?;
        }
        Target::Gui(gui) => {
            ctx.get_gui(gui).await?;
        }
        Target::ProjectManager(project_manager) => {
            ctx.get_project_manager(project_manager).await?;
        }
        Target::Ide(ide) => {
            ctx.get_ide(ide).await?;
        }
    };
    info!("Completed main job.");
    global::complete_tasks().await?;
    Ok(())
}

// #[tokio::test]
// async fn watcher() -> Result {
//     pretty_env_logger::init();
//     debug!("Test is starting!");
//     let mut initial_config = WorkingData::default();
//     initial_config.pathset = vec![r"H:\NBO\enso5".into()];
//     let (tx, rx) = tokio::sync::watch::channel(initial_config.clone());
//     tx.send(initial_config)?;
//     let (errors_tx, errors_rx) = tokio::sync::mpsc::channel(1024);
//     let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(1024);
//
//     let l1 = async move {
//         debug!("Awaiting events");
//         while let Some(msg) = event_rx.recv().await {
//             dbg!(&msg);
//             warn!("{msg:#?}");
//         }
//     };
//
//     tokio::spawn(l1);
//     let worker = watchexec::fs::worker(rx, errors_tx, event_tx);
//     worker.await?;
//     Ok(())
// }

fn main() -> Result {
    let rt = Runtime::new()?;
    rt.block_on(async { main_internal().await })?;
    rt.shutdown_timeout(Duration::from_secs(60 * 30));
    Ok(())
}
