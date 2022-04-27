use crate::cli::arg::Source;
use crate::prelude::*;
use crate::project::project_manager::ProjectManager;
use crate::source_args_hlp;
use clap::Args;

#[derive(Args, Clone, Debug, PartialEq)]
pub struct BuildInput;

source_args_hlp!(ProjectManager, "project-manager", BuildInput);

#[derive(Args, Clone, Debug)]
pub struct Target {
    /// Command for GUI package.
    #[clap(flatten)]
    pub source: Source<ProjectManager>,
}
