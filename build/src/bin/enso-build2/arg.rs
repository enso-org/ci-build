use crate::prelude::*;

use clap::Arg;
use clap::ArgEnum;
use clap::Args;
use clap::Parser;
use clap::Subcommand;
use enso_build::args::BuildKind;
use enso_build::project::gui::Gui;
use enso_build::project::ide::Ide;
use enso_build::project::project_manager::ProjectManager;
use enso_build::project::wasm::Wasm;
use ide_ci::models::config::RepoContext;
use octocrab::models::RunId;

lazy_static! {
    pub static ref DIST_IDE: PathBuf = PathBuf::from_iter(["dist", "ide"]);
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

#[derive(Subcommand, Clone, Debug, PartialEq)]
pub enum IdeCommand {
    Build,
    Watch,
}

#[derive(ArgEnum, Clone, Copy, Debug, PartialEq)]
pub enum TargetSource {
    /// Target will be built from the target repository's sources.
    Build,
    /// Target will be copied from the local path.
    Local,
    CiRun,
    /// bar
    Whatever,
}

pub trait IsTargetSource {
    const SOURCE_NAME: &'static str;
    const PATH_NAME: &'static str;
    const OUTPUT_PATH_NAME: &'static str;
    const RUN_ID_NAME: &'static str;
    const ARTIFACT_NAME_NAME: &'static str;
    const DEFAULT_OUTPUT_PATH: &'static str;
}

macro_rules! source_args_hlp {
    ($target:ident, $prefix:literal) => {
        impl IsTargetSource for $target {
            const SOURCE_NAME: &'static str = concat!($prefix, "-", "source");
            const PATH_NAME: &'static str = concat!($prefix, "-", "path");
            const OUTPUT_PATH_NAME: &'static str = concat!($prefix, "-", "output-path");
            const RUN_ID_NAME: &'static str = concat!($prefix, "-", "run-id");
            const ARTIFACT_NAME_NAME: &'static str = concat!($prefix, "-", "artifact-name");
            const DEFAULT_OUTPUT_PATH: &'static str = concat!("dist/", $prefix);
        }
    };
}

source_args_hlp!(Wasm, "wasm");
source_args_hlp!(Gui, "gui");
source_args_hlp!(ProjectManager, "project-manager");
source_args_hlp!(Ide, "ide");

/// This is the CLI representation of a [`Source`] for a given target.
#[derive(Args, Clone, Debug)]
pub struct TargetSourceArg<Target: IsTargetSource> {
    #[clap(name = Target::SOURCE_NAME, arg_enum, default_value_t = TargetSource::Build, long)]
    pub source: TargetSource,

    /// If source is `local`, this argument is required to give the path.
    #[clap(name = Target::PATH_NAME, long, required_if_eq(Target::SOURCE_NAME, "local"))]
    pub path: Option<PathBuf>,

    /// Directory where artifacts should be placed.
    #[clap(name = Target::RUN_ID_NAME, long, required_if_eq(Target::SOURCE_NAME, "ci-run"))]
    pub run_id: Option<RunId>,

    /// Artifact name to be used when downloading a run artifact. If not set, the default name can
    /// be used.
    #[clap(name = Target::ARTIFACT_NAME_NAME, long)]
    pub artifact_name: Option<String>,

    /// Directory where artifacts should be placed.
    #[clap(name = Target::OUTPUT_PATH_NAME, long)]
    #[clap(parse(try_from_str=normalize_path), default_value=Target::DEFAULT_OUTPUT_PATH)]
    pub output_path: PathBuf,

    #[clap(skip)]
    pub phantom: PhantomData<Target>,
}

#[derive(Args, Clone, Debug)]
pub struct WasmTarget {
    #[clap(flatten)]
    pub wasm: TargetSourceArg<Wasm>,
}

#[derive(Args, Clone, Debug)]
pub struct GuiTarget {
    #[clap(flatten)]
    pub wasm:    TargetSourceArg<Wasm>,
    #[clap(flatten)]
    pub gui:     TargetSourceArg<Gui>,
    /// Command for GUI package.
    #[clap(subcommand)]
    pub command: GuiCommand,
}

#[derive(Args, Clone, Debug)]
pub struct ProjectManagerTarget {
    #[clap(flatten)]
    pub project_manager: TargetSourceArg<ProjectManager>,
}

#[derive(Args, Clone, Debug)]
pub struct IdeTarget {
    #[clap(flatten)]
    pub project_manager: TargetSourceArg<ProjectManager>,
    #[clap(flatten)]
    pub wasm:            TargetSourceArg<Wasm>,
    #[clap(flatten)]
    pub gui:             TargetSourceArg<Gui>,
    #[clap(default_value=DIST_IDE.as_str())]
    pub ide_output_path: PathBuf,
    /// Command for IDE package.
    #[clap(subcommand)]
    pub command:         IdeCommand,
}

#[derive(Subcommand, Clone, Debug)]
pub enum Target {
    Wasm(WasmTarget),
    Gui(GuiTarget),
    /// Build a bundle with Project Manager. Bundle includes Engine and Runtime.
    ProjectManager(ProjectManagerTarget),
    Ide(IdeTarget),
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
