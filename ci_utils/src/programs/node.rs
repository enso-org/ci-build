use crate::prelude::*;

pub struct Node;

impl Program for Node {
    fn executable_name() -> &'static str {
        "node"
    }
}

pub struct Npm;

impl Program for Npm {
    fn executable_name() -> &'static str {
        "npm"
    }
}

impl Npm {
    pub fn install(&self, path: impl AsRef<Path>) -> anyhow::Result<Command> {
        let mut cmd = self.cmd()?;
        cmd.arg("install").current_dir(path);
        Ok(cmd)
    }
}
