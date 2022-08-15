use crate::prelude::*;

use clap::Args;
use clap::Subcommand;

#[derive(Subcommand, Clone, Debug)]
pub enum Action {
    CreateDraft,
    Publish,
}

#[derive(Args, Clone, Debug)]
pub struct Target {
    #[clap(subcommand)]
    pub action: Action,
}
