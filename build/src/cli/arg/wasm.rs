use crate::prelude::*;

use crate::cli::arg::ArgExt;
use crate::cli::arg::OutputPath;
use crate::cli::arg::Source;
use crate::project::wasm::Wasm;
use crate::source_args_hlp;

use clap::ArgEnum;
use clap::Args;
use clap::Subcommand;
use ide_ci::programs::wasm_pack;
use std::lazy::SyncOnceCell;

source_args_hlp!(Wasm, "wasm", BuildInputs);

static DEFAULT_WASM_SIZE_LIMIT: SyncOnceCell<String> = SyncOnceCell::new();

pub fn initialize_default_wasm_size_limit(limit: byte_unit::Byte) -> Result {
    DEFAULT_WASM_SIZE_LIMIT
        .set(limit.get_appropriate_unit(true).to_string())
        .map_err(|e| anyhow!("WASM size limit was already set to {e}."))
}

#[derive(ArgEnum, Clone, Copy, Debug, PartialEq)]
pub enum Profile {
    Dev,
    Profile,
    Release,
}

impl From<Profile> for wasm_pack::Profile {
    fn from(profile: Profile) -> Self {
        match profile {
            Profile::Dev => Self::Dev,
            Profile::Profile => Self::Profile,
            Profile::Release => Self::Release,
        }
    }
}

// Follows hierarchy defined in  lib/rust/profiler/src/lib.rs
#[derive(ArgEnum, Clone, Copy, Debug, PartialEq)]
pub enum ProfilingLevel {
    Objective,
    Task,
    Details,
    Debug,
}

impl From<ProfilingLevel> for crate::project::wasm::ProfilingLevel {
    fn from(profile: ProfilingLevel) -> Self {
        match profile {
            ProfilingLevel::Objective => Self::Objective,
            ProfilingLevel::Task => Self::Task,
            ProfilingLevel::Details => Self::Details,
            ProfilingLevel::Debug => Self::Debug,
        }
    }
}

#[derive(Args, Clone, Debug, PartialEq)]
pub struct BuildInputs {
    /// Which crate should be treated as a WASM entry point. Relative path from source root.
    #[clap(default_value = crate::project::wasm::DEFAULT_TARGET_CRATE, long, enso_env())]
    pub crate_path: PathBuf,

    /// Profile that is passed to wasm-pack.
    #[clap(long, arg_enum, default_value_t = Profile::Release, enso_env())]
    pub wasm_profile: Profile,

    /// Additional options to be passed to Cargo.
    #[clap(last = true, enso_env())]
    pub cargo_options: Vec<String>,

    /// Compiles Enso with given profiling level. If not set, defaults to minimum.
    #[clap(long, arg_enum, enso_env())]
    pub profiling_level: Option<ProfilingLevel>,

    /// Fail the build if compressed WASM exceeds the specified size. Supports format like
    /// "4.06MiB".
    #[clap(long, enso_env(), maybe_default = DEFAULT_WASM_SIZE_LIMIT.get() )]
    pub wasm_size_limit: Option<byte_unit::Byte>,
}

#[derive(Subcommand, Clone, Debug, PartialEq)]
pub enum Command {
    /// Build the WASM package.
    Build {
        #[clap(flatten)]
        params:      BuildInputs,
        #[clap(flatten)]
        output_path: OutputPath<Wasm>,
    },
    /// Lint the coodebase.
    Check,
    /// Get the WASM artifacts from arbitrary source (e.g. release).
    Get {
        #[clap(flatten)]
        source: Source<Wasm>,
    },
    /// Start an ongoing watch process that rebuilds WASM when its sources are touched.
    Watch {
        #[clap(flatten)]
        params:      BuildInputs,
        #[clap(flatten)]
        output_path: OutputPath<Wasm>,
    },
    /// Run the unit tests.
    Test {
        /// Skip the native (non-WASM) Rust tests.
        #[clap(long)]
        no_native: bool,
        /// Skip the WASM Rust tests.
        #[clap(long)]
        no_wasm:   bool,
    },
}

#[derive(Args, Clone, Debug)]
pub struct Target {
    /// Command for WASM part of GUI (aka the Rust part).
    #[clap(subcommand, name = "command")]
    pub command: Command,
}
