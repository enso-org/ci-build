use crate::prelude::*;

use crate::arg::OutputPath;
use crate::arg::Source;
use crate::source_args_hlp;

use clap::Args;
use clap::Subcommand;
use enso_build::project::gui::Gui;
use enso_build::project::project_manager::ProjectManager;

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
        gui:             crate::arg::gui::WatchInput,
        #[clap(flatten)]
        project_manager: Source<ProjectManager>,
    },
}

#[derive(Args, Clone, Debug)]
pub struct Target {
    #[clap(subcommand)]
    pub command: Command,
}
