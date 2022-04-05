use crate::arg::OutputPath;
use crate::arg::Source;
use crate::prelude::*;
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
    Watch {
        #[clap(flatten)]
        gui:             Source<Gui>,
        #[clap(flatten)]
        project_manager: Source<ProjectManager>,
    },
}

#[derive(Args, Clone, Debug)]
pub struct Target {
    // #[clap(flatten)]
    // pub project_manager: TargetSource<ProjectManager>,
    // #[clap(flatten)]
    // pub wasm:            TargetSource<Wasm>,
    // #[clap(flatten)]
    // pub gui:             Source<Gui>,
    // #[clap(flatten)]
    // pub project_manager: Source<ProjectManager>,
    // #[clap(flatten)]
    // pub output_path:     OutputPath<Self>,
    // Command for IDE package.
    #[clap(subcommand)]
    pub command: Command,
}
