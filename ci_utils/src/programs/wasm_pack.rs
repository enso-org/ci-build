use crate::prelude::*;

pub struct WasmPack;

impl Program for WasmPack {
    fn executable_name() -> &'static str {
        "wasm-pack"
    }
}
