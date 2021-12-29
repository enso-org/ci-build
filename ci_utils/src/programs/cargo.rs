use crate::prelude::*;

pub struct Cargo;

impl Program for Cargo {
    fn executable_name() -> &'static str {
        "cargo"
    }
}
