use crate::prelude::*;

use crate::program::command::Manipulator;

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

impl Manipulator for Color {
    fn apply<C: IsCommandWrapper + ?Sized>(&self, command: &mut C) {
        command.args(["--color", self.as_ref()]);
    }
}

#[derive(Clone, PartialEq, Debug, strum::AsRefStr)]
#[strum(serialize_all = "kebab-case")]
pub enum Options {
    Workspace,
    Package(String),
    AllTargets,
}

impl Manipulator for Options {
    fn apply<C: IsCommandWrapper + ?Sized>(&self, command: &mut C) {
        let base_arg = format!("--{}", self.as_ref());
        command.arg(base_arg);
        use Options::*;
        match self {
            Workspace | AllTargets => {}
            Package(package_name) => {
                command.arg(package_name.as_str());
            }
        }
    }
}
