use crate::prelude::*;

use crate::arg::OutputPath;
use crate::arg::Source;
use crate::source_args_hlp;
use clap::ArgEnum;
use clap::Args;
use clap::Subcommand;
use enso_build::project::wasm::Wasm;
use ide_ci::programs::wasm_pack;

source_args_hlp!(Wasm, "wasm", BuildInputs);

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

#[derive(ArgEnum, Clone, Copy, Debug, PartialEq)]
pub enum ProfilingLevel {
    Objective,
    Task,
    Details,
    Debug,
}

impl From<ProfilingLevel> for enso_build::project::wasm::ProfilingLevel {
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
    #[clap(default_value = enso_build::project::wasm::TARGET_CRATE, long)]
    pub crate_path: PathBuf,

    #[clap(long, arg_enum, default_value_t = Profile::Release)]
    pub wasm_profile: Profile,

    #[clap(last = true)]
    pub cargo_options: Vec<String>,

    #[clap(long, arg_enum)]
    pub profiling_level: Option<ProfilingLevel>,
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
    #[clap(subcommand, name = "command")]
    pub command: Command,
}
