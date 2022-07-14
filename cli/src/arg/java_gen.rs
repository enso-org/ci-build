use crate::prelude::*;

use clap::Args;
use clap::Subcommand;

#[derive(Subcommand, Clone, Copy, Debug, PartialEq)]
pub enum Command {
    /// Generate Java.
    Build,
    /// Generate Java and run self-tests.
    Test,
}

#[derive(Args, Clone, Debug)]
pub struct Target {
    #[clap(subcommand)]
    pub action: Command,
}
