use crate::prelude::*;

use crate::arg::OutputPath;
use crate::arg::Source;
use crate::arg::WatchJob;
use crate::source_args_hlp;
use enso_build::project::backend::Backend;
use enso_build::project::gui::Gui;
use enso_build::project::ide;
use enso_build::project::wasm::DEFAULT_INTEGRATION_TESTS_WASM_TIMEOUT;

use clap::Args;
use clap::Subcommand;
use octocrab::models::ReleaseId;

source_args_hlp!(ide::Packed, "ide", BuildInput);
source_args_hlp!(ide::Unpacked, "ide", BuildInput);

#[derive(Args, Clone, Debug, PartialEq)]
pub struct BuildInput {
    #[clap(flatten)]
    pub gui:             Source<Gui>,
    #[clap(flatten)]
    pub project_manager: Source<Backend>,
    #[clap(flatten)]
    pub output_path:     OutputPath<ide::Unpacked>,
    /// Prevents packing the IDE into a single file. Output shall be a directory subtree.
    #[clap(long, enso_env())]
    pub unpacked:        bool,
}

#[derive(Subcommand, Clone, Debug)]
pub enum Command {
    /// Builds both Project Manager and GUI, puts them together into a single, client Electron
    /// application.
    Build {
        #[clap(flatten)]
        params: BuildInput,
    },
    Upload {
        #[clap(flatten)]
        params:     BuildInput,
        #[clap(long, env = enso_build::env::ReleaseId::NAME)]
        release_id: ReleaseId,
    },
    /// Like `Build` but automatically starts the IDE.
    Start {
        #[clap(flatten)]
        params:     BuildInput,
        /// Additional option to be passed to Enso IDE. Can be used multiple times to pass many
        /// arguments.
        #[clap(long, allow_hyphen_values = true, enso_env())]
        ide_option: Vec<String>,
    },
    /// Builds Project Manager and runs it in the background. Builds GUI and runs it using
    /// webpack's dev server.
    Watch {
        #[clap(flatten)]
        gui:             WatchJob<Gui>,
        #[clap(flatten)]
        project_manager: Source<Backend>,
    },
    /// Runs integration tests. This involves building and spawning Project Manager, unless
    /// requested otherwise.
    IntegrationTest {
        /// If set, the project manager won't be spawned.
        #[clap(long)]
        external_backend:  bool,
        #[clap(flatten)]
        project_manager:   Source<Backend>,
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
