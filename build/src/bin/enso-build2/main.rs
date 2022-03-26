#![feature(adt_const_params)]
#![feature(explicit_generic_args_with_impl_trait)]

use anyhow::Context;
use enso_build::prelude::*;
use std::marker::PhantomData;
use std::ops::Deref;

use clap::Arg;
use clap::ArgEnum;
use clap::Args;
use clap::Parser;
use clap::Subcommand;
use enso_build::args::BuildKind;
use enso_build::ide::artifacts::Ide;
use enso_build::ide::pm_provider::ProjectManagerSource;
use enso_build::ide::web::IdeDesktop;
use enso_build::ide::BuildInfo;
use enso_build::paths::generated::RepoRoot;
use enso_build::paths::TargetTriple;
use enso_build::project::gui::Gui;
use enso_build::project::gui::GuiInputs;
use enso_build::project::project_manager::ProjectManager;
use enso_build::project::wasm::Wasm;
use enso_build::project::wasm::WasmSource;
use enso_build::project::IsTarget;
use enso_build::setup_octocrab;
use ide_ci::models::config::RepoContext;
use ide_ci::platform::default_shell;
use ide_ci::programs::tar::Tar;
use ide_ci::programs::Git;
use lazy_static::lazy_static;
use octocrab::models::RunId;

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
            println!("Adding default value {} to argument {}", default.as_ref(), self.get_id());
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

// pub struct RunContext {
//     repo: RepoContext,
//     run:  RunId,
// }

#[derive(Subcommand, Clone, Debug)]
pub enum GuiCommand {
    Build,
    Watch,
}

#[derive(ArgEnum, Clone, Copy, Debug)]
pub enum TargetSource {
    /// Target will be built from the target repository's sources.
    Build,
    /// Target will be copied from the local path.
    Local,
    OngoingCiRun,
    /// bar
    Whatever,
}

// #[derive(Clone, Copy, Debug)]
// pub enum TargetSourceValue {
//     /// Target will be built from the target repository's sources.
//     Build { repo_root: PathBuf },
//     /// Target will be copied from the local path.
//     LocalPath { artifact_path: PathBuf },
//     // /// WASM will be copied from the local path.
//     // CurrentCiRun,
//     /// bar
//     Whatever,
// }
//


// pub struct Foo<> {}

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

    pub async fn get(&self, target: Target, inputs: Target::BuildInput) -> Result<Target::Output>
    where Target: IsTarget + Sync {
        let source = self.resolve()?;
        target.get(source, move || Ok(inputs), self.output_path.clone()).await
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
    // Ide {
    //     #[clap(flatten)]
    //     project_manager_source: TargetSourceArg<Wasm>,
    //
    //     #[clap(flatten)]
    //     gui_source: TargetSourceArg<Wasm>,
    //
    //     /// Where the GUI artifacts should be placed.
    //     #[clap(long, default_value = DIST_IDE.as_str(), parse(try_from_str=normalize_path))]
    //     output_path: PathBuf,
    // },
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



#[tokio::main]
async fn main() -> Result {
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
        Err(e) => Git::new(&cli.repo_path).head_hash().await?,
    };

    let info_for_js = BuildInfo {
        commit,
        name: "Enso IDE".into(),
        version: triple.versions.version.clone(),
        engine_version: triple.versions.version.clone(),
    };

    ////////



    match &cli.target {
        Target::Wasm { wasm_source } => {
            let _wasm = wasm_source.get(Wasm, paths.clone()).await?;
        }
        Target::Gui { gui_source, command, wasm_source } => {
            let wasm = wasm_source.get(Wasm, paths.clone());

            let repo_root = paths.clone();
            let gui_inputs = GuiInputs { wasm: wasm.await?, build_info: info_for_js, repo_root };

            match command {
                GuiCommand::Build => {
                    gui_source.get(Gui, inputs).await?;
                    // let source = gui_source.resolve()?;
                    // let gui = Gui.get(source, inputs, wasm_source.output_path.clone()).await?;
                }
                GuiCommand::Watch => {
                    let gui = Gui.watch(inputs.await?).await?;
                }
            }
        }
        Target::ProjectManager { project_manager_source } => {
            let inputs = enso_build::project::project_manager::Inputs {
                octocrab:  octocrab.clone(),
                versions:  versions.clone(),
                repo_root: paths.path.clone(),
            };
            let _pm = project_manager_source.get(ProjectManager, inputs).await?;
        } /* Target::Ide { output_path, .. } => {
           *     let pm_source = ProjectManagerSource::Local { repo_root: cli.repo_path.clone()
           * };     let triple = triple.clone();
           *     let project_manager = pm_source.get(triple, &paths.dist.project_manager).await?;
           *     let wasm = enso_build::ide::wasm::build_wasm(&cli.repo_path,
           * &paths.dist.wasm).await?;     let web =
           * enso_build::ide::web::IdeDesktop::new(&paths.app.ide_desktop);     let gui
           * = web.build(&wasm, &info_for_js, &paths.dist.content).await?;
           *     let icons = web.build_icons(&paths.dist.icons).await?;
           *     web.dist(&gui, &project_manager, &icons, &output_path).await?;
           * } */
    };

    Ok(())
}
