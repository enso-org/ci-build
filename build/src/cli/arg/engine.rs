use crate::prelude::*;

use crate::cli::arg::Source;
use crate::project::engine::Engine;
use crate::source_args_hlp;
use clap::Args;

source_args_hlp!(Engine, "engine", BuildInput);

#[derive(Args, Clone, Debug, PartialEq)]
pub struct BuildInput {}

#[derive(Args, Clone, Debug)]
pub struct Target {
    #[clap(flatten)]
    pub source: Source<Engine>,
}
