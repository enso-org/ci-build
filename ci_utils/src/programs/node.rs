use crate::prelude::*;

#[derive(Clone, Copy, Debug, Default)]
pub struct Node;

impl Program for Node {
    fn executable_name() -> &'static str {
        "node"
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Npm;

impl Program for Npm {
    fn executable_name() -> &'static str {
        "npm"
    }
}

impl Npm {
    pub fn install(&self, path: impl AsRef<Path>) -> anyhow::Result<Command> {
        // // We must strip any UNC prefix, because CMD does not support having it as a current
        // // directory, and npm is effectively a CMD script wrapping the actual program. See:
        // // https://github.com/npm/cli/issues/3349
        // //
        // // If this becomes an issue, consider toggling `DisableUNCCheck` on win runner machines
        // and // revert this workaround. See also:
        // // https://www.ibm.com/support/pages/disableunccheck-registry-key-created-during-rational-synergy-installation
        // let path = dbg!(path.as_ref().strip_prefix(r"\\?\")).unwrap_or(path.as_ref());
        let mut cmd = self.cmd()?;
        cmd.arg("install").current_dir(path);
        Ok(cmd)
    }
}
