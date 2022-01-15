use crate::prelude::*;

use anyhow::Context;
use std::fmt::Write;
use tokio::process::Child;

pub trait CommandExt {
    fn run_ok(&mut self) -> BoxFuture<'static, Result<()>>;

    // TODO: `spawn` but does logs like some other methods. They all need a naming unification pass.
    fn spawn_nicer(&mut self) -> Result<Child>;

    fn describe(&self) -> String;
}

impl CommandExt for Command {
    fn run_ok(&mut self) -> BoxFuture<'static, Result<()>> {
        let pretty = self.describe();
        println!("Will run: {}", pretty);
        let status = self.status();
        async move { status.await?.exit_ok().context(format!("Command failed: {}", pretty)) }
            .boxed()
    }

    fn spawn_nicer(&mut self) -> Result<Child> {
        let pretty = self.describe();
        println!("Spawning {}", pretty);
        self.spawn().context(format!("Failed to spawn: {}", pretty))
    }

    fn describe(&self) -> String {
        let mut ret = String::new();
        let pretty_printed = format!("{:?}", self.as_std());
        let _ = writeln!(ret, "Command:\n\t{}", pretty_printed);
        if let Some(cwd) = self.as_std().get_current_dir() {
            let _ = writeln!(ret, "\twith working directory: {}", cwd.display());
        };
        let env = self.as_std().get_envs();
        if !env.is_empty() {
            let _ = writeln!(ret, "\twith environment overrides:");
        }
        for (name, val) in self.as_std().get_envs() {
            let _ = writeln!(
                ret,
                "\t\t{}={}",
                name.to_string_lossy(),
                val.map_or(default(), OsStr::to_string_lossy)
            );
        }
        ret
        // ?self.as_std().get_program()
    }
}
