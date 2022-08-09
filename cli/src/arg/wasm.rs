use enso_build::prelude::*;

use crate::arg::ArgExt;
use crate::arg::Source;
use crate::arg::WatchJob;
use crate::source_args_hlp;
use crate::BuildJob;
use crate::IsWatchableSource;

use clap::ArgEnum;
use clap::Args;
use clap::Subcommand;
use enso_build::project::wasm::Wasm;
use std::lazy::SyncOnceCell;

pub use enso_build::project::wasm::Profile;

source_args_hlp!(Wasm, "wasm", BuildInput);

impl IsWatchableSource for Wasm {
    type WatchInput = WatchInput;
}

static DEFAULT_WASM_SIZE_LIMIT: SyncOnceCell<String> = SyncOnceCell::new();

pub fn initialize_default_wasm_size_limit(limit: byte_unit::Byte) -> Result {
    DEFAULT_WASM_SIZE_LIMIT
        .set(limit.get_appropriate_unit(true).to_string())
        .map_err(|e| anyhow!("WASM size limit was already set to {e}."))
}

// Follows hierarchy defined in  lib/rust/profiler/src/lib.rs
#[derive(ArgEnum, Clone, Copy, Debug, PartialEq)]
pub enum ProfilingLevel {
    Objective,
    Task,
    Detail,
    Debug,
}

impl From<ProfilingLevel> for enso_build::project::wasm::ProfilingLevel {
    fn from(profile: ProfilingLevel) -> Self {
        match profile {
            ProfilingLevel::Objective => Self::Objective,
            ProfilingLevel::Task => Self::Task,
            ProfilingLevel::Detail => Self::Detail,
            ProfilingLevel::Debug => Self::Debug,
        }
    }
}

#[derive(Args, Clone, Debug, PartialEq)]
pub struct BuildInput {
    /// Which crate should be treated as a WASM entry point. Relative path from source root.
    #[clap(default_value = enso_build::project::wasm::DEFAULT_TARGET_CRATE, long, enso_env())]
    pub crate_path: PathBuf,

    /// Profile that is passed to wasm-pack.
    #[clap(long, arg_enum, default_value_t = Profile::Release, enso_env())]
    pub wasm_profile: Profile,

    /// Additional options to be passed to wasm-opt. Might overwrite the optimization flag
    /// resulting from 'wasm_profile' setting.
    #[clap(long, allow_hyphen_values = true, enso_env())]
    pub wasm_opt_option: Vec<String>,

    /// Do not invoke wasm-opt, even if it is part of current profile.
    #[clap(long, conflicts_with = "wasm-opt-option", enso_env())]
    pub skip_wasm_opt: bool,

    /// Additional options to be passed to Cargo.
    #[clap(last = true, enso_env())]
    pub cargo_options: Vec<String>,

    /// Compiles Enso with given profiling level. If not set, defaults to minimum.
    #[clap(long, arg_enum, enso_env())]
    pub profiling_level: Option<ProfilingLevel>,

    /// Fail the build if compressed WASM exceeds the specified size. Supports format like
    /// "4.06MiB". Pass "0" to disable check.
    #[clap(long, enso_env(), maybe_default = DEFAULT_WASM_SIZE_LIMIT.get() )]
    pub wasm_size_limit: Option<byte_unit::Byte>,
}

#[derive(Args, Clone, Debug, PartialEq)]
pub struct WatchInput {
    /// Additional option to be passed to Cargo. Can be used multiple times to pass many arguments.
    #[clap(long, allow_hyphen_values = true, enso_env())]
    pub cargo_watch_option: Vec<String>,
}

#[derive(Subcommand, Clone, Debug, PartialEq)]
pub enum Command {
    /// Build the WASM package.
    Build(BuildJob<Wasm>),
    /// Lint the coodebase.
    Check,
    /// Get the WASM artifacts from arbitrary source (e.g. release).
    Get(Source<Wasm>),
    /// Start an ongoing watch process that rebuilds WASM when its sources are touched.
    Watch(WatchJob<Wasm>),
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
