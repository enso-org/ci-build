use crate::prelude::*;

pub mod gui;
pub mod ide;
pub mod project_manager;
pub mod wasm;

use clap::Arg;
use clap::ArgEnum;
use clap::Args;
use clap::Parser;
use clap::Subcommand;
use enso_build::args::BuildKind;
use ide_ci::models::config::RepoContext;
use octocrab::models::RunId;

lazy_static! {
    pub static ref DIST_IDE: PathBuf = PathBuf::from_iter(["dist", "ide"]);
    pub static ref DEFAULT_REPO_PATH: Option<String> =
        enso_build::repo::deduce_repository_path().map(|p| p.display().to_string());
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


/// We pass CLI paths through this to make sure that they are absolutized against the initial
/// working directory, not whatever it will be set to later.
pub fn normalize_path(path: &str) -> Result<PathBuf> {
    let ret = PathBuf::from(path);
    let ret = ret.absolutize()?;
    Ok(ret.to_path_buf())
}

#[derive(ArgEnum, Clone, Copy, Debug, PartialEq)]
pub enum TargetSourceKind {
    /// Target will be built from the target repository's sources.
    Build,
    /// Target will be copied from the local path.
    Local,
    CiRun,
    /// bar
    Whatever,
}

#[derive(Args, Clone, Copy, Debug, PartialEq)]
pub struct NoArgs {}

pub trait IsTargetSource {
    const SOURCE_NAME: &'static str;
    const PATH_NAME: &'static str;
    const OUTPUT_PATH_NAME: &'static str;
    const RUN_ID_NAME: &'static str;
    const RELEASE_DESIGNATOR_NAME: &'static str;
    const ARTIFACT_NAME_NAME: &'static str;
    const DEFAULT_OUTPUT_PATH: &'static str;

    type BuildInput: Args + Send + Sync = NoArgs;
}

#[macro_export]
macro_rules! source_args_hlp {
    ($target:ty, $prefix:literal, $inputs:ty) => {
        impl $crate::arg::IsTargetSource for $target {
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
    /// Rust part of the GUI that is compiled to WASM.
    Wasm(wasm::Target),

    /// GUI that consists of WASM and JS parts. This is what we depy to cloud.
    Gui(gui::Target),

    /// Project Manager bundle, that includes Enso Engine with GraalVM Runtime.
    ProjectManager(project_manager::Target),

    /// IDE bundles together GUI and Project Manager bundle.
    Ide(ide::Target),
    Clean,
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


/// Describe where to get a target artifacts from.
///
/// This is the CLI representation of a [`Source`] for a given target.
#[derive(Args, Clone, Debug, PartialEq)]
pub struct Source<Target: IsTargetSource> {
    #[clap(name = Target::SOURCE_NAME, arg_enum, long, default_value_t= SourceKind::Build)]
    pub source: SourceKind,

    /// If source is `local`, this argument is required to give the path.
    #[clap(name = Target::PATH_NAME, long, required_if_eq(Target::SOURCE_NAME, "local"))]
    pub path: Option<PathBuf>,

    /// Identifier of the CI run from which the artifacts should be downloaded.
    #[clap(name = Target::RUN_ID_NAME, long, required_if_eq(Target::SOURCE_NAME, "ci-run"))]
    pub run_id: Option<RunId>,

    /// Artifact name to be used when downloading a run artifact. If not set, the default name can
    /// be used.
    #[clap(name = Target::ARTIFACT_NAME_NAME, long)]
    pub artifact_name: Option<String>,

    #[clap(name = Target::RELEASE_DESIGNATOR_NAME, long, required_if_eq(Target::SOURCE_NAME, "release"))]
    pub release: Option<String>,

    #[clap(flatten)]
    pub build_args: Target::BuildInput,

    #[clap(flatten)]
    pub output_path: OutputPath<Target>,
    //
    // #[clap(skip)]
    // pub phantom: PhantomData<Target>,
}

#[derive(ArgEnum, Clone, Copy, Debug, PartialEq)]
pub enum SourceKind {
    Build,
    Local,
    CiRun,
    Release,
}

#[derive(Args, Clone, Debug, PartialEq)]
pub struct OutputPath<Target: IsTargetSource> {
    /// Directory where artifacts should be placed.
    #[clap(name = Target::OUTPUT_PATH_NAME, long)]
    #[clap(parse(try_from_str=normalize_path), default_value=Target::DEFAULT_OUTPUT_PATH)]
    pub output_path: PathBuf,
    #[clap(skip)]
    pub phantom:     PhantomData<Target>,
}

impl<Target: IsTargetSource> AsRef<Path> for OutputPath<Target> {
    fn as_ref(&self) -> &Path {
        self.output_path.as_path()
    }
}


// #[cfg(test)]
// mod tests {
//     use super::*;
//
//     use enso_build::project::IsTarget;
//
//     pub fn parse_source<Target: IsTarget + IsTargetSource>(
//         text: &str,
//     ) -> Result<crate::Source<Target>> {
//         todo!()
//     }
// }
