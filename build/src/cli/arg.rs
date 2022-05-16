use crate::prelude::*;

pub mod gui;
pub mod ide;
pub mod project_manager;
pub mod wasm;

use crate::args::BuildKind;

use clap::Arg;
use clap::ArgEnum;
use clap::Args;
use clap::Parser;
use clap::Subcommand;
use ide_ci::cache;
use ide_ci::models::config::RepoContext;
use octocrab::models::RunId;

/// The prefix that will be used when reading the build script arguments from environment.
pub const ENVIRONMENT_VARIABLE_NAME_PREFIX: &str = "ENSO_BUILD";

pub const DEFAULT_REMOTE_REPOSITORY: &str = "enso-org/enso";

pub fn default_repo_path() -> Option<PathBuf> {
    crate::repo::deduce_repository_path()
}

pub fn default_repo_remote() -> RepoContext {
    ide_ci::actions::env::GITHUB_REPOSITORY
        .get()
        .unwrap_or_else(|_| RepoContext::from_str("DEFAULT_REMOTE_REPOSITORY").unwrap())
}

pub fn default_cache_path() -> Option<PathBuf> {
    cache::default_path().ok()
}

/// Extensions to the `clap::Arg`, intended to be used as argument attributes.
pub trait ArgExt<'h>: Sized + 'h {
    /// Allow setting argument through an environment variable prefixed with Enso Build name.
    fn enso_env(self) -> Self;
}

impl<'h> ArgExt<'h> for Arg<'h> {
    fn enso_env(self) -> Self {
        self.prefixed_env(ENVIRONMENT_VARIABLE_NAME_PREFIX)
    }
}

/// We pass CLI paths through this to make sure that they are absolutized against the initial
/// working directory, not whatever it will be set to later.
pub fn normalize_path(path: &str) -> Result<PathBuf> {
    let ret = PathBuf::from(path);
    let ret = ret.absolutize()?;
    Ok(ret.to_path_buf())
}

/// Collection of strings used by CLI that are specific to a given target.
pub trait IsTargetSource {
    const SOURCE_NAME: &'static str;
    const PATH_NAME: &'static str;
    const OUTPUT_PATH_NAME: &'static str;
    const RUN_ID_NAME: &'static str;
    const RELEASE_DESIGNATOR_NAME: &'static str;
    const ARTIFACT_NAME_NAME: &'static str;
    const DEFAULT_OUTPUT_PATH: &'static str;

    type BuildInput: Debug + Args + Send + Sync;
}

#[macro_export]
macro_rules! source_args_hlp {
    ($target:ty, $prefix:literal, $inputs:ty) => {
        impl $crate::cli::arg::IsTargetSource for $target {
            const SOURCE_NAME: &'static str = concat!($prefix, "-", "source");
            const PATH_NAME: &'static str = concat!($prefix, "-", "path");
            const OUTPUT_PATH_NAME: &'static str = concat!($prefix, "-", "output-path");
            const RUN_ID_NAME: &'static str = concat!($prefix, "-", "run-id");
            const RELEASE_DESIGNATOR_NAME: &'static str = concat!($prefix, "-", "release");
            const ARTIFACT_NAME_NAME: &'static str = concat!($prefix, "-", "artifact-name");
            const DEFAULT_OUTPUT_PATH: &'static str = concat!("dist/", $prefix);

            type BuildInput = $inputs;
        }
    };
}

#[derive(Subcommand, Clone, Debug)]
pub enum Target {
    /// Build/Test the Rust part of the GUI.
    Wasm(wasm::Target),
    /// Build/Run GUI that consists of WASM and JS parts. This is what we deploy to cloud.
    Gui(gui::Target),
    /// Build/Get Project Manager bundle (includes Enso Engine with GraalVM Runtime).
    ProjectManager(project_manager::Target),
    /// Build/Run/Test IDE bundle (includes GUI and Project Manager).
    Ide(ide::Target),
    /// Clean the repository. Keeps the IntelliJ's .idea directory intact.
    Clean,
    /// Lint the codebase.
    Lint,
    /// Apply automatic formatters on the repository.
    Fmt,
}

/// Build, test and package Enso Engine.
#[derive(Clone, Debug, Parser)]
#[clap(author, version, about, long_about = None)]
pub struct Cli {
    /// Path to the directory with sources to be built, typically the root of the 'enso'
    /// repository's working copy.
    #[clap(long, maybe_default_os = default_repo_path(), enso_env())]
    pub repo_path: PathBuf,

    /// Where build script will cache some of the third-party artifacts (like network downloads).
    #[clap(long, maybe_default_os = default_cache_path(), enso_env())]
    pub cache_path: PathBuf,

    /// The GitHub repository with the project. This is mainly used to manage releases (checking
    /// released versions to generate a new one, or uploading release assets).
    /// The argument should follow the format `owner/repo_name`.
    #[clap(long, default_value = "enso/enso-staging", enso_env())] // FIXME
    pub repo_remote: RepoContext,

    /// The build kind. Affects the default version generation.
    #[clap(long, arg_enum, default_value_t = BuildKind::Dev, enso_env())]
    pub build_kind: BuildKind,

    #[clap(long, default_value_t = TARGET_OS, enso_env(), possible_values=[OS::Windows.as_str(), OS::Linux.as_str(), OS::MacOS.as_str()])]
    pub target_os: OS,

    #[clap(subcommand)]
    pub target: Target,
}

// pub fn parse_os(text: &str) -> Result<OS> {
//     use std::str::FromStr;
//     OS::from_str(text).anyhow_err()
// }

/// Describe where to get a target artifacts from.
///
/// This is the CLI representation of a [crate::source::Source] for a given target.
#[derive(Args, Clone, Debug, PartialEq)]
pub struct Source<Target: IsTargetSource> {
    /// How the given target should be acquired.
    #[clap(name = Target::SOURCE_NAME, arg_enum, long, default_value_t= SourceKind::Build, enso_env())]
    pub source: SourceKind,

    /// If source is `local`, this argument is used to give the path with the component.
    /// If missing, the default would-be output directory for this component shall be used.
    #[clap(name = Target::PATH_NAME, long, default_value=Target::DEFAULT_OUTPUT_PATH, enso_env())]
    pub path: PathBuf,

    /// If source is `run`, this argument is required to provide CI run ID.
    #[clap(name = Target::RUN_ID_NAME, long, required_if_eq(Target::SOURCE_NAME, "ci-run"), enso_env())]
    pub run_id: Option<RunId>,

    /// Artifact name to be used when downloading a run artifact. If not set, the default name for
    /// given target will be used.
    #[clap(name = Target::ARTIFACT_NAME_NAME, long, enso_env())]
    pub artifact_name: Option<String>,

    /// If source is `run`, this argument is required to identify a release with asset to download.
    /// This can be either the release tag or a predefined placeholder (currently supported one is
    /// only 'latest').
    #[clap(name = Target::RELEASE_DESIGNATOR_NAME, long, required_if_eq(Target::SOURCE_NAME, "release"), enso_env())]
    pub release: Option<String>,

    #[clap(flatten)]
    pub build_args: Target::BuildInput,

    #[clap(flatten)]
    pub output_path: OutputPath<Target>,
}

/// Discriminator denoting how some target artifact should be obtained.
#[derive(ArgEnum, Clone, Copy, Debug, PartialEq)]
pub enum SourceKind {
    /// Target will be built from the target repository's sources.
    Build,
    /// Already built target will be copied from the local path.
    Local,
    /// Target will be downloaded from a completed CI run artifact.
    CiRun,
    /// Target will be downloaded from the CI run that is currently executing this script.
    CurrentCiRun,
    /// Target will be downloaded from a release asset.
    Release,
}

/// Strongly typed argument for an output directory of a given build target.
#[derive(Args, Clone, PartialEq)]
pub struct OutputPath<Target: IsTargetSource> {
    /// Directory where artifacts should be placed.
    #[clap(name = Target::OUTPUT_PATH_NAME, long, parse(try_from_str=normalize_path), default_value = Target::DEFAULT_OUTPUT_PATH, enso_env())]
    pub output_path: PathBuf,
    #[clap(skip)]
    pub phantom:     PhantomData<Target>,
}

impl<Target: IsTargetSource> Debug for OutputPath<Target> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.output_path.fmt(f)
    }
}

impl<Target: IsTargetSource> AsRef<Path> for OutputPath<Target> {
    fn as_ref(&self) -> &Path {
        self.output_path.as_path()
    }
}
