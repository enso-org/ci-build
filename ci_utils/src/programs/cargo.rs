use crate::prelude::*;
use crate::program::command::CommandOption;

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

/// Control when colored output is used.
#[derive(Clone, Copy, PartialEq, Debug, strum::AsRefStr)]
#[strum(serialize_all = "kebab-case")]
pub enum Color {
    /// Never display colors.
    None,
    /// Always display colors.
    Always,
    /// Automatically detect if color support is available on the terminal.
    Auto,
}

impl CommandOption for Color {
    fn args(&self) -> Vec<&str> {
        vec!["--color", self.as_ref()]
    }
}
