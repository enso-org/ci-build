use enso_build::prelude::*;

use crate::arg::Source;
use crate::source_args_hlp;
use clap::Args;
use clap::Subcommand;
use enso_build::project::backend::Backend;

#[derive(Args, Clone, Debug, PartialEq)]
pub struct BuildInput {
    // #[clap(flatten)]
    // pub project_manager: Source<project::project_manager::ProjectManager>,
    // #[clap(flatten)]
    // pub engine:          Source<project::engine::Engine>,
}

source_args_hlp!(Backend, "backend", BuildInput);

#[derive(Subcommand, Clone, Debug, PartialEq)]
pub enum Command {
    /// Gets the GUI, either by compiling it from scratch or downloading from an external source.
    Get {
        #[clap(flatten)]
        source: Source<Backend>,
    },
    /// Continuously rebuilds GUI when its sources are changed and serves it using dev-server.
    Upload {
        #[clap(flatten)]
        input: BuildInput,
    },
    /// Execute benchmarks.
    Benchmark {
        #[clap(arg_enum)]
        which: Vec<enso_build::engine::Benchmarks>,
    },
    CiCheck {},
}

#[derive(Args, Clone, Debug, PartialEq)]
pub struct Target {
    /// Command for GUI package.
    #[clap(subcommand)]
    pub command: Command,
}
