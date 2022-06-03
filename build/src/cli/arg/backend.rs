use crate::cli::arg::Source;
use crate::prelude::*;
use crate::project;
use crate::project::backend::Backend;
use crate::source_args_hlp;
use clap::Args;

#[derive(Args, Clone, Debug, PartialEq)]
pub struct BuildInput {
    #[clap(flatten)]
    pub project_manager: Source<project::project_manager::ProjectManager>,
    #[clap(flatten)]
    pub engine:          Source<project::engine::Engine>,
}

source_args_hlp!(Backend, "backend", BuildInput);

#[derive(Args, Clone, Debug)]
pub struct Target {
    /// Command for GUI package (i.e. Rust + JS content).
    #[clap(flatten)]
    pub source: Source<Backend>,
}
