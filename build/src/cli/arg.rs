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

pub const ENVIRONMENT_VARIABLE_NAME_PREFIX: &str = "ENSO_BUILD";

lazy_static! {
    pub static ref DIST_IDE: PathBuf = PathBuf::from_iter(["dist", "ide"]);
    pub static ref DEFAULT_REPO_PATH: Option<String> =
        crate::repo::deduce_repository_path().map(|p| p.display().to_string());
    pub static ref DEFAULT_CACHE_PATH: Option<String> =
        cache::default_path().ok().map(|p| p.display().to_string());
}

pub trait ArgExt<'h>: Sized + 'h {
    fn maybe_default<S: AsRef<str> + 'h>(self, f: &'h impl Deref<Target = Option<S>>) -> Self;
    fn prefixed_env(self) -> Self;
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

    fn prefixed_env(self) -> Self {
        use heck::ToShoutySnakeCase;
        let var_name = format!(
            "{}_{}",
            ENVIRONMENT_VARIABLE_NAME_PREFIX,
            self.get_id().to_shouty_snake_case()
        );
        // println!("Argument {arg_name} will be covered by {var_name} variable");
        // FIXME don't leak, provide some unified solution for "static generated String" case
        self.env(Box::leak(var_name.into_boxed_str()))
    }
}
//
//
// pub trait AppExt<'h>: Sized + 'h {
//     fn env_args(self, prefix: &str) -> Self;
// }
//
// impl<'h> AppExt<'h> for clap::Command<'h> {
//     fn env_args(mut self, prefix: &str) -> Self {
//         println!("Processing {}", self.get_name());
//         let arg_names = self
//             .get_arguments()
//             .map(|a| a.get_id())
//             .filter(|a| *a != "version" && *a != "help")
//             .collect::<Vec<_>>();
//
//         for arg_name in arg_names {
//             use heck::ToShoutySnakeCase;
//             let var_name = format!("ENSO_BUILD_{}", arg_name.to_shouty_snake_case());
//             println!("Argument {arg_name} will be covered by {var_name} variable");
//             self = self.mut_arg(arg_name, |arg| arg.env(Box::leak(var_name.into_boxed_str())));
//         }
//         self
//     }
// }



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
    /// Already built target will be copied from the local path.
    Local,
    /// Target will be downloaded from a CI run artifact.
    CiRun,
    /// TODO
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

    type BuildInput: Debug + Args + Send + Sync = NoArgs;
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
    /// Rust part of the GUI that is compiled to WASM.
    Wasm(wasm::Target),
    /// GUI that consists of WASM and JS parts. This is what we deploy to cloud.
    Gui(gui::Target),
    /// Project Manager bundle (includes Enso Engine with GraalVM Runtime).
    ProjectManager(project_manager::Target),
    /// IDE bundles together GUI and Project Manager bundle.
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
    /// repository.
    #[clap(long, maybe_default = &DEFAULT_REPO_PATH)]
    pub repo_path: PathBuf,

    /// Where build script will cache some of the third-party artifacts (like network downloads).
    #[clap(long, maybe_default = &DEFAULT_CACHE_PATH)]
    pub cache_path: PathBuf,

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
    /// How the given target should be acquired.
    #[clap(name = Target::SOURCE_NAME, arg_enum, long, default_value_t= SourceKind::Build)]
    pub source: SourceKind,

    /// If source is `local`, this argument is used to give the path with the component.
    /// If missing, the default would-be output directory for this component shall be used.
    #[clap(name = Target::PATH_NAME, long, default_value=Target::DEFAULT_OUTPUT_PATH)]
    pub path: PathBuf,

    /// If source is `run`, this argument is required to provide CI run ID.
    #[clap(name = Target::RUN_ID_NAME, long, required_if_eq(Target::SOURCE_NAME, "ci-run"))]
    pub run_id: Option<RunId>,

    /// Artifact name to be used when downloading a run artifact. If not set, the default name for
    /// given target will be used.
    #[clap(name = Target::ARTIFACT_NAME_NAME, long)]
    pub artifact_name: Option<String>,

    /// If source is `run`, this argument is required to identify a release with asset to download.
    /// This can be either the release tag or a predefined placeholder (currently supported one is
    /// only 'latest').
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
    CurrentCiRun,
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
//     use crate::project::IsTarget;
//
//     pub fn parse_source<Target: IsTarget + IsTargetSource>(
//         text: &str,
//     ) -> Result<crate::Source<Target>> {
//         todo!()
//     }
// }
