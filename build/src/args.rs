use crate::prelude::*;

use strum::EnumString;

#[derive(FromArgs, Clone, PartialEq, Debug)]
#[argh(subcommand)]
pub enum WhatToDo {
    Build(Build),
    // Three release-related commands below.
    Prepare(Prepare),
    Upload(Upload),
    Finish(Finish),
    // Utilities
    Run(Run),
}

/// Just build the packages.
#[derive(FromArgs, Clone, PartialEq, Debug)]
#[argh(subcommand, name = "build")]
pub struct Build {}

/// Create a new draft release on GitHub and emit relevant information to the CI environment.
#[derive(FromArgs, Clone, PartialEq, Debug)]
#[argh(subcommand, name = "prepare")]
pub struct Prepare {}

/// Build all the release packages and bundles and upload them to GitHub release. Must run with
/// environment adjusted by the `prepare` command.
#[derive(FromArgs, Clone, PartialEq, Debug)]
#[argh(subcommand, name = "upload")]
pub struct Upload {}

/// Publish the release.  Must run with environment adjusted by the `prepare` command. Typically
/// called once after platform-specific `upload` commands are done.
#[derive(FromArgs, Clone, PartialEq, Debug)]
#[argh(subcommand, name = "finish")]
pub struct Finish {}

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

/// Build, test and packave Enso Engine.
#[derive(Clone, Debug, FromArgs)]
pub struct Args {
    /// build kind (dev/nightly)
    #[argh(option, default = "default_kind()")]
    pub kind:       BuildKind,
    /// path to the local copy of the Enso Engine repository
    #[argh(positional)]
    pub repository: PathBuf,
    /// identifier of the release to be targeted (necessary for `upload` and `finish` commands)
    #[argh(option)]
    pub release_id: Option<u64>,
    /// whether create bundles with Project Manager and Launcher
    #[argh(option)]
    pub bundle:     Option<bool>,
    /// command to execute (build/prepare/upload/finish)
    #[argh(subcommand)]
    pub command:    WhatToDo,
    /* #[argh(subcommand)]
     * pub task:       Vec<Task>, */
}
