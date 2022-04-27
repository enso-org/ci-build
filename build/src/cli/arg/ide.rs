use crate::prelude::*;

use crate::cli::arg::OutputPath;
use crate::cli::arg::Source;
use crate::source_args_hlp;

use crate::project::gui::Gui;
use crate::project::project_manager::ProjectManager;
use clap::Args;
use clap::Subcommand;

source_args_hlp!(Target, "ide", BuildInput);

#[derive(Args, Clone, Debug)]
pub struct BuildInput {
    #[clap(flatten)]
    pub gui:             Source<Gui>,
    #[clap(flatten)]
    pub project_manager: Source<ProjectManager>,
    #[clap(flatten)]
    pub output_path:     OutputPath<Target>,
}

#[derive(Subcommand, Clone, Debug)]
pub enum Command {
    Build {
        #[clap(flatten)]
        params: BuildInput,
    },
    Start {
        #[clap(flatten)]
        params: BuildInput,
    },
    Watch {
        #[clap(flatten)]
        gui:             crate::cli::arg::gui::WatchInput,
        #[clap(flatten)]
        project_manager: Source<ProjectManager>,
    },
    IntegrationTest {
        /// If set, the project manager won't be spawned.
        #[clap(long)]
        external_backend: bool,
        #[clap(flatten)]
        project_manager:  Source<ProjectManager>,
    },
}

#[derive(Args, Clone, Debug)]
pub struct Target {
    #[clap(subcommand)]
    pub command: Command,
}
