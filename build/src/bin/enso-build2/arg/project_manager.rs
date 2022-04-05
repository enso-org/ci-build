use crate::arg::Source;
use crate::prelude::*;
use crate::source_args_hlp;
use clap::Args;
use enso_build::project::project_manager::ProjectManager;

#[derive(Args, Clone, Debug, PartialEq)]
pub struct BuildInput;

source_args_hlp!(ProjectManager, "project-manager", BuildInput);

#[derive(Args, Clone, Debug)]
pub struct Target {
    /// Command for GUI package.
    #[clap(flatten)]
    pub source: Source<ProjectManager>,
}
