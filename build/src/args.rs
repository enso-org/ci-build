use crate::prelude::*;

use ide_ci::models::config::RepoContext;
use strum::EnumString;

#[derive(FromArgs, Clone, PartialEq, Debug, strum::Display)]
#[argh(subcommand)]
pub enum WhatToDo {
    Build(Build),
    // Three release-related commands below.
    Create(Create),
    Upload(Upload),
    Publish(Publish),
    // Utilities
    Run(Run),
}

impl WhatToDo {
    pub fn is_release_command(&self) -> bool {
        use WhatToDo::*;
        // Not using matches! to force manual check when extending enum.
        match self {
            Create(_) => true,
            Upload(_) => true,
            Publish(_) => true,
            Build(_) | Run(_) => false,
        }
    }
}

/// Just build the packages.
#[derive(FromArgs, Clone, PartialEq, Debug)]
#[argh(subcommand, name = "build")]
pub struct Build {}

/// Create a new draft release on GitHub and emit relevant information to the CI environment.
#[derive(FromArgs, Clone, PartialEq, Debug)]
#[argh(subcommand, name = "prepare releaqse")]
pub struct Create {}

/// Build all the release packages and bundles and upload them to GitHub release. Must run with
/// environment adjusted by the `prepare` command.
#[derive(FromArgs, Clone, PartialEq, Debug)]
#[argh(subcommand, name = "upload")]
pub struct Upload {}

/// Publish the release.  Must run with environment adjusted by the `prepare` command. Typically
/// called once after platform-specific `upload` commands are done.
#[derive(FromArgs, Clone, PartialEq, Debug)]
#[argh(subcommand, name = "finish")]
pub struct Publish {}

/// Run an arbitrary command with the build environment set (like `PATH`).
#[derive(FromArgs, Clone, PartialEq, Debug)]
#[argh(subcommand, name = "run")]
pub struct Run {
    #[argh(positional)]
    pub command_pieces: Vec<OsString>,
}

#[derive(Clone, PartialEq, Debug, EnumString)]
#[strum(serialize_all = "kebab-case")]
pub enum BuildKind {
    Dev,
    Nightly,
}

pub fn default_kind() -> BuildKind {
    crate::env::build_kind().unwrap_or(BuildKind::Dev)
}

pub fn default_repo() -> Option<RepoContext> {
    ide_ci::actions::env::repository().ok()
}

pub fn parse_repo_context(value: &str) -> std::result::Result<Option<RepoContext>, String> {
    RepoContext::from_str(value).map(Some).map_err(|e| e.to_string())
}

/// Build, test and packave Enso Engine.
#[derive(Clone, Debug, FromArgs)]
pub struct Args {
    /// build kind (dev/nightly)
    #[argh(option, default = "default_kind()")]
    pub kind:       BuildKind,
    /// path to the local copy of the Enso Engine repository
    #[argh(positional)]
    pub target:     PathBuf,
    /// identifier of the release to be targeted (necessary for `upload` and `finish` commands)
    #[argh(option)]
    pub release_id: Option<u64>,
    /// whether create bundles with Project Manager and Launcher
    #[argh(option)]
    pub bundle:     Option<bool>,
    /// repository that will be targeted for the release info purposes
    #[argh(option, from_str_fn(parse_repo_context), default = "default_repo()")]
    pub repo:       Option<RepoContext>,
    #[argh(subcommand)]
    pub command:    WhatToDo,
    /* #[argh(subcommand)]
     * pub task:       Vec<Task>, */
}
