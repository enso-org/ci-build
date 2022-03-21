use enso_build::prelude::*;
use std::ops::Deref;

use clap::Arg;
use clap::ArgEnum;
use clap::Parser;
use clap::Subcommand;
use enso_build::args::BuildKind;
use enso_build::ide::wasm::build_wasm;
use enso_build::ide::wasm::download_wasm_from_run;
use enso_build::ide::BuildInfo;
use enso_build::paths::generated::Parameters;
use enso_build::paths::generated::Paths;
use enso_build::paths::generated::PathsRepoRootDistWasm;
use enso_build::paths::TargetTriple;
use enso_build::setup_octocrab;
use ide_ci::models::config::RepoContext;
use ide_ci::programs::Git;
use lazy_static::lazy_static;
use octocrab::models::RunId;


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

pub struct RunContext {
    repo: RepoContext,
    run:  RunId,
}

#[derive(Subcommand, Clone, Debug)]
pub enum GuiCommand {
    Build,
    Watch,
}

#[derive(ArgEnum, Clone, Debug)]
pub enum WasmSource {
    /// WASM will be built from the target repository's sources.
    Build,
    /// WASM will be copied from the local path.
    LocalPath,
    /// bar
    Whatever,
}

#[derive(Subcommand, Clone, Debug)]
pub enum Target {
    Wasm {
        /// Where the WASM artifacts should be placed.
        #[clap(default_value = "dist/wasm", parse(try_from_str=normalize_path))]
        output_path: PathBuf,
    },
    Gui {
        /// Where the GUI artifacts should be placed.
        #[clap(long, default_value = "dist/gui", parse(try_from_str=normalize_path))]
        output_path: PathBuf,

        #[clap(long, arg_enum, default_value_t = WasmSource::Whatever)]
        wasm_source: WasmSource,

        #[clap(long, required_if_eq("wasm-source", "local-path"))]
        wasm_bundle: Option<PathBuf>,

        /// Command for GUI package.
        #[clap(subcommand)]
        command: GuiCommand,
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
    /* /// build kind (dev/nightly)
     * #[argh(option, default = "default_kind()")]
     * pub kind:       BuildKind,
     * /// path to the local copy of the Enso Engine repository
     * #[argh(positional)]
     * pub target:     PathBuf,
     * /// identifier of the release to be targeted (necessary for
     * `upload` and `finish` commands)
     * #[argh(option)] pub release_id:
     * Option<u64>,
     * /// repository that will be targeted for the release info purposes
     * #[argh(option, from_str_fn(parse_repo_context), default =
     * "default_repo()")] pub repo:
     * Option<RepoContext>, // #[argh(subcommand)]
     * // pub command:    WhatToDo,
     * /* #[argh(subcommand)]
     *  * pub task:       Vec<Task>, */ */
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
        Target::Gui { output_path, command, .. } => {
            let wasm =
                enso_build::ide::wasm::build_wasm(&cli.repo_path, &paths.repo_root.dist.wasm)
                    .await?;
            let web = enso_build::ide::web::IdeDesktop::new(&paths.repo_root.app.ide_desktop);
            match command {
                GuiCommand::Build => {
                    web.build(&wasm, &info_for_js, output_path).await?;
                }
                GuiCommand::Watch => {
                    web.watch(&wasm, &info_for_js).await?;
                }
            }
        }
    };

    Ok(())
}
