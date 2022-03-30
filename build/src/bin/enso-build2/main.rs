#![feature(explicit_generic_args_with_impl_trait)]

use anyhow::Context;
use enso_build::prelude::*;
use std::any::type_name;
use std::ops::Deref;
use std::time::Duration;

use clap::Arg;
use clap::ArgEnum;
use clap::Args;
use clap::Parser;
use clap::Subcommand;
use enso_build::args::BuildKind;
use enso_build::paths::TargetTriple;
use enso_build::project::gui::BuildInfo;
use enso_build::project::gui::Gui;
use enso_build::project::gui::GuiInputs;
use enso_build::project::ide::Ide;
use enso_build::project::project_manager::ProjectManager;
use enso_build::project::wasm::Wasm;
use enso_build::project::IsTarget;
use enso_build::setup_octocrab;
use ide_ci::actions::workflow::is_in_env;
use ide_ci::global;
use ide_ci::models::config::RepoContext;
use ide_ci::programs::Git;
use lazy_static::lazy_static;
use tokio::runtime::Runtime;

lazy_static! {
    pub static ref DIST_WASM: PathBuf = PathBuf::from_iter(["dist", "wasm"]);
    pub static ref DIST_GUI: PathBuf = PathBuf::from_iter(["dist", "gui"]);
    pub static ref DIST_IDE: PathBuf = PathBuf::from_iter(["dist", "ide"]);
    pub static ref DIST_PROJECT_MANAGER: PathBuf = PathBuf::from_iter(["dist", "project-manager"]);
}

pub trait ArgExt<'h>: Sized + 'h {
    fn maybe_default<S: AsRef<str> + 'h>(self, f: &'h impl Deref<Target = Option<S>>) -> Self;
}

impl<'h> ArgExt<'h> for Arg<'h> {
    fn maybe_default<S: AsRef<str> + 'h>(self, f: &'h impl Deref<Target = Option<S>>) -> Self {
        if let Some(default) = f.deref().as_ref() {
            debug!("Adding default value {} to argument {}", default.as_ref(), self.get_id());
            self.default_value(default.as_ref()).required(false)
        } else {
            self
        }
    }
}

lazy_static! {
    static ref DEFAULT_REPO_PATH: Option<String> =
        enso_build::repo::deduce_repository_path().map(|p| p.display().to_string());
}

/// We pass CLI paths through this to make sure that they are absolutized against the initial
/// working directory, not whatever it will be set to later.
pub fn normalize_path(path: &str) -> Result<PathBuf> {
    let ret = PathBuf::from(path);
    let ret = ret.absolutize()?;
    Ok(ret.to_path_buf())
}

#[derive(Subcommand, Clone, Debug, PartialEq)]
pub enum GuiCommand {
    Build,
    Watch,
}

#[derive(ArgEnum, Clone, Copy, Debug, PartialEq)]
pub enum TargetSource {
    /// Target will be built from the target repository's sources.
    Build,
    /// Target will be copied from the local path.
    Local,
    OngoingCiRun,
    /// bar
    Whatever,
}

pub trait IsTargetSource {
    const SOURCE_NAME: &'static str;
    const PATH_NAME: &'static str;
    const OUTPUT_PATH_NAME: &'static str;
    const DEFAULT_OUTPUT_PATH: &'static str;
}

impl IsTargetSource for Wasm {
    const SOURCE_NAME: &'static str = "wasm-source";
    const PATH_NAME: &'static str = "wasm-path";
    const OUTPUT_PATH_NAME: &'static str = "wasm-output-path";
    const DEFAULT_OUTPUT_PATH: &'static str = "dist/wasm";
}

impl IsTargetSource for Gui {
    const SOURCE_NAME: &'static str = "gui-source";
    const PATH_NAME: &'static str = "gui-path";
    const OUTPUT_PATH_NAME: &'static str = "gui-output-path";
    const DEFAULT_OUTPUT_PATH: &'static str = "dist/gui";
}

impl IsTargetSource for ProjectManager {
    const SOURCE_NAME: &'static str = "project-manager-source";
    const PATH_NAME: &'static str = "project-manager-path";
    const OUTPUT_PATH_NAME: &'static str = "project-manager-output-path";
    const DEFAULT_OUTPUT_PATH: &'static str = "dist/project-manager";
}

impl IsTargetSource for Ide {
    const SOURCE_NAME: &'static str = "ide-source";
    const PATH_NAME: &'static str = "ide-path";
    const OUTPUT_PATH_NAME: &'static str = "ide-output-path";
    const DEFAULT_OUTPUT_PATH: &'static str = "dist/ide";
}

#[derive(Args, Clone, Debug)]
pub struct TargetSourceArg<Target: IsTargetSource> {
    #[clap(name = Target::SOURCE_NAME, arg_enum, default_value_t = TargetSource::Build, long)]
    source: TargetSource,

    /// If source is `local`, this argument is required to give the path.
    #[clap(name = Target::PATH_NAME, long, required_if_eq(Target::SOURCE_NAME, "local"))]
    path: Option<PathBuf>,

    /// Directory where artifacts should be placed.
    #[clap(name = Target::OUTPUT_PATH_NAME, long)]
    #[clap(parse(try_from_str=normalize_path), default_value=Target::DEFAULT_OUTPUT_PATH)]
    output_path: PathBuf,

    #[clap(skip)]
    phantom: PhantomData<Target>,
}

impl<Target: IsTargetSource> TargetSourceArg<Target> {
    pub fn resolve(&self) -> Result<enso_build::project::Source> {
        Ok(match self.source {
            TargetSource::Build => enso_build::project::Source::BuildLocally,
            TargetSource::Local => enso_build::project::Source::LocalFile(
                self.path.clone().context("Missing path to the local WASM artifacts!")?,
            ),
            TargetSource::OngoingCiRun => enso_build::project::Source::OngoingCiRun,
            TargetSource::Whatever => {
                todo!()
            }
        })
    }

    pub fn get(
        &self,
        target: Target,
        inputs: Target::BuildInput,
    ) -> impl Future<Output = Result<Target::Output>> + 'static
    where
        Target: IsTarget + Sync + 'static,
    {
        let output_path = self.output_path.clone();
        let source = self.resolve();
        // We upload only built artifacts. There would be no point in uploading something that we've
        // just downloaded.
        let should_upload_artifact = self.source == TargetSource::Build && is_in_env();
        async move {
            info!("Getting target {}.", type_name::<Target>());
            let artifact = target.get(source?, move || Ok(inputs), output_path).await?;
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
    }
}

#[derive(Subcommand, Clone, Debug)]
pub enum Target {
    Wasm {
        #[clap(flatten)]
        wasm_source: TargetSourceArg<Wasm>,
    },
    Gui {
        #[clap(flatten)]
        wasm_source: TargetSourceArg<Wasm>,
        #[clap(flatten)]
        gui_source:  TargetSourceArg<Gui>,
        /// Command for GUI package.
        #[clap(subcommand)]
        command:     GuiCommand,
    },
    /// Build a bundle with Project Manager. Bundle includes Engine and Runtime.
    ProjectManager {
        #[clap(flatten)]
        project_manager_source: TargetSourceArg<ProjectManager>,
    },
    Ide {
        #[clap(flatten)]
        project_manager_source: TargetSourceArg<ProjectManager>,
        #[clap(flatten)]
        wasm_source:            TargetSourceArg<Wasm>,
        #[clap(flatten)]
        gui_source:             TargetSourceArg<Gui>,
        #[clap(flatten)]
        ide_source:             TargetSourceArg<Ide>,
    },
}

/// Build, test and packave Enso Engine.
#[derive(Clone, Debug, Parser)]
#[clap(author, version, about, long_about = None)]
pub struct Cli {
    /// Path to the directory with sources to be built, typically the root of the 'enso'
    /// repository.
    #[clap(long, maybe_default = &DEFAULT_REPO_PATH)]
    pub repo_path: PathBuf,

    /// The GitHub repository with the project. This is mainly used to manage releases (checking
    /// released versions to generate a new one, or uploading release assets).
    /// The argument should follow the format `owner/repo_name`.
    #[clap(long, default_value = "enso/enso-staging")] // FIXME
    pub repo_remote: RepoContext,

    /// The build kind. Affects the default version generation.
    #[clap(long, arg_enum, default_value_t = BuildKind::Dev)]
    pub build_kind: BuildKind,

    #[clap(subcommand)]
    pub target: Target,
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
    pub remote_repo: Option<RepoContext>,
}

// impl BuildContext {
//     pub fn new(cli: &Cli) {
//         let octocrab = setup_octocrab()?;
//         let versions = enso_build::version::deduce_versions(
//             &octocrab,
//             BuildKind::Dev,
//             Some(&cli.repo_remote),
//             &cli.repo_path,
//         )
//         .await?;
//         let triple = TargetTriple::new(versions);
//         triple.versions.publish()?;
//         Self { source_root: cli.repo_path.clone() }
//     }
// }


async fn main_internal() -> Result {
    console_subscriber::init();
    pretty_env_logger::init();
    debug!("Setting up.");
    DEFAULT_REPO_PATH.as_ref().map(|path| path.as_str());
    let cli = Cli::try_parse()?;
    dbg!(&cli);

    /////////
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

    let paths = enso_build::paths::generated::RepoRoot::new(&cli.repo_path, triple.to_string());

    let commit = match ide_ci::actions::env::Sha.fetch() {
        Ok(commit) => commit,
        Err(_e) => Git::new(&cli.repo_path).head_hash().await?,
    };

    let info_for_js = BuildInfo {
        commit,
        name: "Enso IDE".into(),
        version: triple.versions.version.clone(),
        engine_version: triple.versions.version.clone(),
    };

    let pm_inputs = enso_build::project::project_manager::Inputs {
        octocrab:  octocrab.clone(),
        versions:  triple.versions.clone(),
        repo_root: paths.path.clone(),
    };

    match &cli.target {
        Target::Wasm { wasm_source } => {
            let _wasm = wasm_source.get(Wasm, paths.clone()).await?;
        }
        Target::Gui { gui_source, command, wasm_source } => {
            let wasm = wasm_source.get(Wasm, paths.clone());

            let repo_root = paths.clone();
            let inputs = GuiInputs { wasm: wasm.boxed(), build_info: info_for_js, repo_root };

            match command {
                GuiCommand::Build => {
                    gui_source.get(Gui, inputs).await?;
                    // let source = gui_source.resolve()?;
                    // let gui = Gui.get(source, inputs, wasm_source.output_path.clone()).await?;
                }
                GuiCommand::Watch => {
                    let _gui = Gui.watch(inputs).await?;
                }
            }
        }
        Target::ProjectManager { project_manager_source } => {
            project_manager_source.get(ProjectManager, pm_inputs).await?;
        }
        Target::Ide { ide_source, wasm_source, gui_source, project_manager_source } => {
            let wasm = wasm_source.get(Wasm, paths.clone());
            let gui_inputs = GuiInputs {
                wasm:       wasm.boxed(),
                build_info: info_for_js,
                repo_root:  paths.clone(),
            };
            let ide_inputs = enso_build::project::ide::BuildInput {
                repo_root:       paths.clone(),
                version:         gui_inputs.build_info.version.clone(),
                gui:             gui_source.get(Gui, gui_inputs).boxed(),
                project_manager: project_manager_source.get(ProjectManager, pm_inputs).boxed(),
            };
            let output_path = ide_source.output_path.clone();
            let ide = Ide.build(ide_inputs, output_path).await?;
            ide.upload().await?;
        }
    };
    info!("Completed main job.");
    global::complete_tasks().await?;
    Ok(())
}

fn main() -> Result {
    let rt = Runtime::new()?;
    rt.block_on(async { main_internal().await })?;
    rt.shutdown_timeout(Duration::from_secs(60 * 30));
    Ok(())
}
