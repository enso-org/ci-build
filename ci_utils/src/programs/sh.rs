use crate::prelude::*;

pub struct Sh;

impl Program for Sh {
    fn executable_name() -> &'static str {
        "sh"
    }
}

pub struct Bash;

impl Program for Bash {
    fn executable_name() -> &'static str {
        "bash"
    }
}

impl Shell for Bash {
    fn run_command(&self) -> Result<Command> {
        let mut cmd = Bash.cmd()?;
        cmd.arg("-c");
        Ok(cmd)
    }

    fn run_script(&self, script_path: impl AsRef<Path>) -> Result<Command> {
        let mut cmd = Bash.cmd()?;
        cmd.arg(script_path.as_ref());
        Ok(cmd)
    }

    fn run_shell(&self) -> Result<Command> {
        self.cmd()
    }
}
