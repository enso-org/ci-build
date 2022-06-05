use crate::arg::Source;
use crate::source_args_hlp;
use clap::Args;
use enso_build::prelude::*;
use enso_build::project;
use enso_build::project::backend::Backend;

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
