use crate::prelude::*;

use crate::cli::arg::OutputPath;
use crate::cli::arg::Source;
use crate::project::gui::Gui;
use crate::project::wasm::Wasm;
use crate::source_args_hlp;

use clap::Args;
use clap::Subcommand;

source_args_hlp!(Gui, "gui", BuildInput);

#[derive(Args, Clone, Debug, PartialEq)]
pub struct BuildInput {
    #[clap(flatten)]
    pub wasm: Source<Wasm>,
}

#[derive(Args, Clone, Debug, PartialEq)]
pub struct WatchInput {
    #[clap(flatten)]
    pub wasm:        Source<Wasm>,
    #[clap(flatten)]
    pub output_path: OutputPath<Gui>,
}

#[derive(Subcommand, Clone, Debug, PartialEq)]
pub enum Command {
    /// Builds the GUI from the local sources.
    Build {
        #[clap(flatten)]
        input:       BuildInput,
        #[clap(flatten)]
        output_path: OutputPath<Gui>,
    },
    /// Gets the GUI, either by compiling it from scratch or downloading from an external source.
    Get {
        #[clap(flatten)]
        source: Source<Gui>,
    },
    /// Continuously rebuilds GUI when its sources are changed and serves it using dev-server.
    Watch {
        #[clap(flatten)]
        input: WatchInput,
    },
}

#[derive(Args, Clone, Debug)]
pub struct Target {
    /// Command for GUI package.
    #[clap(subcommand)]
    pub command: Command,
}
