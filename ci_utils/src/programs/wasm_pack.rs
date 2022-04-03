use crate::prelude::*;

use crate::new_command_type;
use tempfile::TempDir;

use crate::programs::Cargo;

pub struct WasmPack;

impl Program for WasmPack {
    type Command = WasmPackCommand;
    fn executable_name() -> &'static str {
        "wasm-pack"
    }
}



new_command_type! {WasmPack, WasmPackCommand}

impl WasmPackCommand {
    pub fn build(mut self) -> WasmPackBuildCommand {
        self.arg("build");
        self.into_inner().into()
    }
}

new_command_type! {WasmPack, WasmPackBuildCommand}

pub async fn install_if_missing() -> Result {
    let temp = TempDir::new()?;
    // We want to run this command in a temporary directory, as to install wasm-pack using a
    // system-wide default toolchain, rather than overrides for the current folder (which is likely
    // under our repository root).
    //
    // Note that this will install the tool to the default system-wide location, not temp.
    if WasmPack.lookup().is_err() {
        Cargo.cmd()?.args(["install", "wasm-pack"]).current_dir(&temp.path()).run_ok().await?;
        // TODO
        //  this kind of function likely could use some generalization, that should also cover how
        //  PATH updates are handled
    }
    Ok(())
}
