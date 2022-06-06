use crate::prelude::*;

use clap::Args;
use clap::Subcommand;
use enso_build::version::BuildKind;

#[derive(Subcommand, Clone, Debug)]
pub enum Action {
    CreateDraft,
    Publish,
}

#[derive(Args, Clone, Debug)]
pub struct Target {
    /// build kind (dev/nightly)
    #[clap(long, arg_enum, enso_env())]
    pub kind: BuildKind,

    #[clap(subcommand)]
    pub action: Action,
}
