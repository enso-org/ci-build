use crate::prelude::*;

use crate::cli::arg::OutputPath;
use crate::cli::arg::Source;
use crate::project::gui::Gui;
use crate::project::project_manager::ProjectManager;
use crate::project::wasm::DEFAULT_INTEGRATION_TESTS_WASM_TIMEOUT;
use crate::source_args_hlp;

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
        external_backend:  bool,
        #[clap(flatten)]
        project_manager:   Source<ProjectManager>,
        /// Run WASM tests in the headless mode
        #[clap(long, parse(try_from_str), default_value_t = true)]
        headless:          bool,
        /// Custom timeout for wasm-bindgen test runner. Supports formats like "300secs" or "5min".
        #[clap(long, default_value_t = DEFAULT_INTEGRATION_TESTS_WASM_TIMEOUT.into())]
        wasm_timeout:      humantime::Duration,
        /// Additional options to be appended to the wasm-pack invocation. Note that wasm-pack will
        /// further redirect any unrecognized option to the underlying cargo call.
        #[clap(last = true)]
        wasm_pack_options: Vec<String>,
    },
}

#[derive(Args, Clone, Debug)]
pub struct Target {
    #[clap(subcommand)]
    pub command: Command,
}
