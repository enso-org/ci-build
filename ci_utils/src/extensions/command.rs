use crate::prelude::*;

use anyhow::Context;

pub trait CommandExt {
    fn run_ok(&mut self) -> BoxFuture<'static, Result<()>>;

    // fn describe(&self) -> String;
}

impl CommandExt for Command {
    fn run_ok(&mut self) -> BoxFuture<'static, Result<()>> {
        let pretty_printed = format!("{:?}", self.as_std());
        println!("Spawning command:\n\t{}", pretty_printed);
        if let Some(cwd) = self.as_std().get_current_dir() {
            println!("\twith working directory: {}", cwd.display())
        }
        let status = self.status();
        async move { status.await?.exit_ok().context(format!("Command: {}", pretty_printed)) }
            .boxed()
    }

    // fn describe(&self) -> String {
    //     default()
    //     // ?self.as_std().get_program()
    // }
}
