use crate::prelude::*;

use strum::EnumString;

#[derive(Clone, PartialEq, Debug, EnumString)]
#[strum(serialize_all = "kebab-case")]
pub enum WhatToDo {
    Build,
    // Three release-related commands below.
    Prepare,
    Upload,
    Finish,
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
    #[argh(positional, default = "WhatToDo::Build")]
    pub command:    WhatToDo,
    /* #[argh(subcommand)]
     * pub task:       Vec<Task>, */
}
