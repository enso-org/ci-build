use crate::prelude::*;
use crate::program::command::Manipulator;

#[derive(Clone, Copy, Debug, strum::Display)]
pub enum OptimizationLevel {
    /// execute default optimization passes (equivalent to -Os)
    O,
    ///  execute no optimization passes
    O0,
    /// execute -O1 optimization passes (quick&useful opts, useful for iteration builds)
    O1,
    /// execute -O2 optimization passes (most opts, generally gets most perf)
    O2,
    /// execute -O3 optimization passes (spends potentially a lot of time optimizing)
    O3,
    /// execute -O4 optimization passes (also flatten the IR, which can take a lot more time and
    /// memory, but is useful on more nested / complex / less-optimized input)
    O4,
    /// execute default optimization passes, focusing on code size
    Os,
    /// execute default optimization passes, super-focusing on code size
    Oz,
}

impl Manipulator for OptimizationLevel {
    fn apply<C: IsCommandWrapper + ?Sized>(&self, command: &mut C) {
        let flag = format!("-{self}");
        command.arg(flag);
    }
}

pub struct Output<'a>(pub &'a Path);

impl Manipulator for Output<'_> {
    fn apply<C: IsCommandWrapper + ?Sized>(&self, command: &mut C) {
        command.arg("-o").arg(&self.0);
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WasmOpt;

impl Program for WasmOpt {
    fn executable_name(&self) -> &str {
        "wasm-opt"
    }
}
