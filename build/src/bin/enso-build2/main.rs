#![feature(adt_const_params)]

use anyhow::Context;
use enso_build::prelude::*;
use std::ops::Deref;

use clap::Arg;
use clap::ArgEnum;
use clap::Args;
use clap::Parser;
use clap::Subcommand;
use enso_build::args::BuildKind;
use enso_build::ide::pm_provider::ProjectManagerSource;
use enso_build::ide::wasm::WasmSource;
use enso_build::ide::web::IdeDesktop;
use enso_build::ide::BuildInfo;
use enso_build::paths::TargetTriple;
use enso_build::setup_octocrab;
use ide_ci::models::config::RepoContext;
use ide_ci::platform::default_shell;
use ide_ci::programs::Git;
use lazy_static::lazy_static;
use octocrab::models::RunId;

lazy_static! {
    pub static ref DIST_WASM: PathBuf = PathBuf::from_iter(["dist", "ide"]);
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
    LocalPath,
    // /// WASM will be copied from the local path.
    // CurrentCiRun,
    /// bar
    Whatever,
}

#[derive(Clone, Copy, Debug)]
pub enum TargetSource2 {
    /// Target will be built from the target repository's sources.
    Build,
    /// Target will be copied from the local path.
    LocalPath,
    // /// WASM will be copied from the local path.
    // CurrentCiRun,
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

#[derive(Args, Clone, Debug)]
pub struct TargetSourceArg<const SOURCE_NAME: &'static str, const PATH_NAME: &'static str> {
    #[clap(long, arg_enum, default_value_t = TargetSource::Whatever, name  = SOURCE_NAME, long = SOURCE_NAME)]
    source: TargetSource,

    /// If source is `local-path`, this argument is required to give the path.
    #[clap(long, required_if_eq(SOURCE_NAME, "local-path"), name  = PATH_NAME, long = PATH_NAME)]
    path: Option<PathBuf>,
}

impl<const SOURCE_NAME: &'static str, const PATH_NAME: &'static str>
    TargetSourceArg<SOURCE_NAME, PATH_NAME>
{
    pub fn resolve(&self, repo_root: impl Into<PathBuf>) -> Result<WasmSource> {
        let repo_root = repo_root.into();
        Ok(match self.source {
            TargetSource::Build => WasmSource::Build { repo_root },
            TargetSource::LocalPath => WasmSource::Local(
                self.path.clone().context("Missing path to the local WASM artifacts!")?,
            ),
            TargetSource::Whatever => WasmSource::Build { repo_root },
        })
    }
}

#[derive(Subcommand, Clone, Debug)]
pub enum Target {
    Wasm {
        /// Where the WASM artifacts should be placed.
        #[clap(default_value = DIST_WASM.as_str(), parse(try_from_str=normalize_path))]
        output_path: PathBuf,
    },
    Gui {
        /// Where the GUI artifacts should be placed.
        #[clap(long, default_value = DIST_GUI.as_str(), parse(try_from_str=normalize_path))]
        output_path: PathBuf,

        /// Command for GUI package.
        #[clap(subcommand)]
        command: GuiCommand,

        #[clap(flatten)]
        wasm_source: TargetSourceArg<"wasm-source", "wasm-path">,
    },
    /// Build a bundle with Project Manager. Bundle includes Engine and Runtime.
    ProjectManager {
        /// Where the GUI artifacts should be placed.
        #[clap(long, default_value = DIST_PROJECT_MANAGER.as_str(), parse(try_from_str=normalize_path))]
        output_path: PathBuf,
    },
    Ide {
        #[clap(flatten)]
        project_manager_source: TargetSourceArg<"project-manager-source", "project-manager-path">,

        #[clap(flatten)]
        gui_source: TargetSourceArg<"gui-source", "gui-path">,

        /// Where the GUI artifacts should be placed.
        #[clap(long, default_value = DIST_IDE.as_str(), parse(try_from_str=normalize_path))]
        output_path: PathBuf,
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



#[tokio::main]
async fn main() -> Result {
    DEFAULT_REPO_PATH.as_ref().map(|path| path.as_str());

    let cli = Cli::try_parse()?;
    dbg!(&cli);

    /////////
    let temp = tempfile::tempdir()?;
    let octocrab = setup_octocrab()?;
    let build_kind = BuildKind::Dev;
    let versions = enso_build::version::deduce_versions(
        &octocrab,
        build_kind,
        Some(&cli.repo_remote),
        &cli.repo_path,
    )
    .await?;
    let triple = TargetTriple::new(versions);
    triple.versions.publish()?;

    //let temp = temp.path();
    let params = enso_build::paths::generated::Parameters {
        repo_root: cli.repo_path.clone(),
        temp:      temp.path().to_owned(),
        triple:    triple.to_string().into(),
    };

    dbg!(&params);
    let paths = enso_build::paths::generated::Paths::new(&params, &PathBuf::from("."));

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
        Target::Wasm { output_path } => {
            // FIXME rebase output path
            enso_build::ide::wasm::build_wasm(&cli.repo_path, &paths.repo_root.dist.wasm).await?;
        }
        Target::Gui { output_path, command, wasm_source } => {
            let wasm_source = wasm_source.resolve(&paths.repo_root)?;
            let wasm = wasm_source.place_at(&octocrab, &paths.repo_root.dist.wasm).await?;
            let web = IdeDesktop::from(&paths);
            match command {
                GuiCommand::Build => {
                    web.build(&wasm, &info_for_js, output_path).await?;
                }
                GuiCommand::Watch => {
                    web.watch(&wasm, &info_for_js).await?;
                }
            }
        }
        Target::ProjectManager { output_path, .. } => {
            let pm_source = ProjectManagerSource::Local { repo_root: cli.repo_path.clone() };
            let triple = triple.clone();
            pm_source.get(triple, output_path).await?;
        }
        Target::Ide { output_path, .. } => {
            let pm_source = ProjectManagerSource::Local { repo_root: cli.repo_path.clone() };
            let triple = triple.clone();
            let project_manager =
                pm_source.get(triple, &paths.repo_root.dist.project_manager).await?;

            let wasm =
                enso_build::ide::wasm::build_wasm(&cli.repo_path, &paths.repo_root.dist.wasm)
                    .await?;


            let web = enso_build::ide::web::IdeDesktop::new(&paths.repo_root.app.ide_desktop);

            let gui = web.build(&wasm, &info_for_js, &paths.repo_root.dist.content).await?;
            let icons = web.build_icons(&paths.repo_root.dist.icons).await?;
            web.dist(&gui, &project_manager, &icons, &paths.repo_root.dist.client).await?;
        }
    };

    Ok(())
}
