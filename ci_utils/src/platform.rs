use crate::prelude::*;

pub mod win;

pub type DefaultShell = impl Shell;

pub fn default_shell() -> DefaultShell {
    #[cfg(not(target_os = "windows"))]
    return crate::programs::Bash;

    #[cfg(target_os = "windows")]
    return crate::programs::Cmd;
}
