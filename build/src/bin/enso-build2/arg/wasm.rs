use crate::prelude::*;

use crate::arg::OutputPath;
use crate::arg::Source;
use crate::source_args_hlp;
use clap::Args;
use clap::Subcommand;
use enso_build::project::wasm::Wasm;

source_args_hlp!(Wasm, "wasm", BuildInputs);

#[derive(Args, Clone, Debug, PartialEq)]
pub struct BuildInputs {
    /// Which crate should be treated as a WASM entry point. Relative path from source root.
    #[clap(default_value=enso_build::project::wasm::TARGET_CRATE,long)]
    pub crate_path: PathBuf,

    #[clap(last = true)]
    pub cargo_options: Vec<String>,
}

#[derive(Subcommand, Clone, Debug, PartialEq)]
pub enum Command {
    Build {
        #[clap(flatten)]
        params:      BuildInputs,
        #[clap(flatten)]
        output_path: OutputPath<Wasm>,
    },
    Check,
    Get {
        #[clap(flatten)]
        source: Source<Wasm>,
    },
    Watch {
        #[clap(flatten)]
        params:      BuildInputs,
        #[clap(flatten)]
        output_path: OutputPath<Wasm>,
    },
    Test {
        #[clap(long)]
        no_native: bool,
        #[clap(long)]
        no_wasm:   bool,
    },
}

#[derive(Args, Clone, Debug)]
pub struct Target {
    /// Command for GUI package.
    #[clap(subcommand)]
    pub command: Command,
}
