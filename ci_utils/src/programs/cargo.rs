use crate::prelude::*;

#[derive(Clone, Copy, Debug, Default)]
pub struct Cargo;

impl Program for Cargo {
    fn init_command<'a>(&self, cmd: &'a mut Self::Command) -> &'a mut Self::Command {
        cmd.args(["--color", "always"])
    }
    fn executable_name() -> &'static str {
        "cargo"
    }
}
