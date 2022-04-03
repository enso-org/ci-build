use crate::prelude::*;
use anyhow::Context;
// use command_group::AsyncCommandGroup;
use std::borrow::BorrowMut;
use std::fmt::Debug;
use std::fmt::Formatter;
use std::process::ExitStatus;
use std::process::Output;
use std::process::Stdio;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncRead;
use tokio::io::BufReader;
use tokio::process::Child;
use tokio::task::JoinHandle;

#[macro_export]
macro_rules! new_command_type {
    ($program_name:ident, $command_name:ident) => {
        #[derive(Shrinkwrap)]
        #[shrinkwrap(mutable)]
        pub struct $command_name(pub $crate::program::command::Command);

        impl From<$crate::program::command::Command> for $command_name {
            fn from(inner: $crate::program::command::Command) -> Self {
                $command_name(inner)
            }
        }

        impl From<$command_name> for $crate::program::command::Command {
            fn from(inner: $command_name) -> Self {
                inner.0
            }
        }

        impl $command_name {
            pub fn into_inner(self) -> $crate::program::command::Command {
                self.0
            }
        }

        impl $crate::program::command::IsCommandWrapper for $command_name {
            fn borrow_mut_command(&mut self) -> &mut tokio::process::Command {
                self.0.borrow_mut_command()
            }
        }

        impl $crate::program::command::MyCommand<$program_name> for $command_name {}
    };
    () => {
        new_command_type!(Command);
    };
}



pub trait MyCommand<P: Program>: BorrowMut<Command> + From<Command> + Into<Command> {
    fn new_program<S: AsRef<OsStr>>(program: S) -> Self {
        let inner = tokio::process::Command::new(program);
        let inner = Command::new_over::<P>(inner);
        Self::from(inner)
    }
}

pub trait IsCommandWrapper {
    fn borrow_mut_command(&mut self) -> &mut tokio::process::Command;

    fn arg<S: AsRef<OsStr>>(&mut self, arg: S) -> &mut Self {
        self.borrow_mut_command().arg(arg);
        self
    }

    fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>, {
        self.borrow_mut_command().args(args);
        self
    }

    fn env<K, V>(&mut self, key: K, val: V) -> &mut Self
    where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>, {
        self.borrow_mut_command().env(key, val);
        self
    }

    fn envs<I, K, V>(&mut self, vars: I) -> &mut Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>, {
        self.borrow_mut_command().envs(vars);
        self
    }

    fn env_remove<K: AsRef<OsStr>>(&mut self, key: K) -> &mut Self {
        self.borrow_mut_command().env_remove(key);
        self
    }

    fn env_clear(&mut self) -> &mut Self {
        self.borrow_mut_command().env_clear();
        self
    }

    fn current_dir<Pa: AsRef<Path>>(&mut self, dir: Pa) -> &mut Self {
        self.borrow_mut_command().current_dir(dir);
        self
    }

    fn stdin<T: Into<Stdio>>(&mut self, cfg: T) -> &mut Self {
        self.borrow_mut_command().stdin(cfg);
        self
    }

    fn stdout<T: Into<Stdio>>(&mut self, cfg: T) -> &mut Self {
        self.borrow_mut_command().stdout(cfg);
        self
    }

    fn stderr<T: Into<Stdio>>(&mut self, cfg: T) -> &mut Self {
        self.borrow_mut_command().stderr(cfg);
        self
    }

    fn kill_on_drop(&mut self, kill_on_drop: bool) -> &mut Self {
        self.borrow_mut_command().kill_on_drop(kill_on_drop);
        self
    }

    #[cfg(windows)]
    #[cfg_attr(docsrs, doc(cfg(windows)))]
    fn creation_flags(&mut self, flags: u32) -> &mut Self {
        self.borrow_mut_command().creation_flags(flags);
        self
    }

    #[cfg(unix)]
    #[cfg_attr(docsrs, doc(cfg(unix)))]
    fn uid(&mut self, id: u32) -> &mut Self {
        self.borrow_mut_command().uid(id);
        self
    }

    #[cfg(unix)]
    #[cfg_attr(docsrs, doc(cfg(unix)))]
    fn gid(&mut self, id: u32) -> &mut Self {
        self.borrow_mut_command().gid(id);
        self
    }

    // fn spawn(&mut self) -> Result<Child> {
    //     self.borrow_mut_command().spawn().anyhow_err()
    // }
    //
    //
    // fn status(&mut self) -> BoxFuture<'static, Result<ExitStatus>> {
    //     let fut = self.borrow_mut_command().status();
    //     async move { fut.await.anyhow_err() }.boxed()
    // }
    //
    // fn output(&mut self) -> BoxFuture<'static, Result<Output>> {
    //     let fut = self.borrow_mut_command().output();
    //     async move { fut.await.anyhow_err() }.boxed()
    // }
}

impl<T: BorrowMut<tokio::process::Command>> IsCommandWrapper for T {
    fn borrow_mut_command(&mut self) -> &mut tokio::process::Command {
        self.borrow_mut()
    }
}

impl<P: Program> MyCommand<P> for Command {
    fn new_program<S: AsRef<OsStr>>(program: S) -> Self {
        let inner = tokio::process::Command::new(program);
        Self::new_over::<P>(inner)
    }
}

pub trait CommandOption {
    fn arg(&self) -> Option<&str> {
        None
    }
    fn args(&self) -> Vec<&str> {
        vec![]
    }
}

pub struct Command {
    pub inner:          tokio::process::Command,
    pub status_checker: Arc<dyn Fn(ExitStatus) -> Result + Send + Sync>,
}

impl Borrow<tokio::process::Command> for Command {
    fn borrow(&self) -> &tokio::process::Command {
        &self.inner
    }
}

impl BorrowMut<tokio::process::Command> for Command {
    fn borrow_mut(&mut self) -> &mut tokio::process::Command {
        &mut self.inner
    }
}

impl Debug for Command {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.inner)
    }
}

impl Command {
    pub fn new<S: AsRef<OsStr>>(program: S) -> Command {
        let inner = tokio::process::Command::new(program);
        let status_checker = Arc::new(|status: ExitStatus| status.exit_ok().anyhow_err());
        Self { inner, status_checker }
    }

    pub fn new_over<P: Program + 'static>(inner: tokio::process::Command) -> Self {
        Command { inner, status_checker: Arc::new(P::handle_exit_status) }
    }

    pub fn run_ok(&mut self) -> BoxFuture<'static, Result<()>> {
        use command_group::tokio::AsyncCommandGroup;

        self.stdout(Stdio::piped());
        self.stderr(Stdio::piped());

        let pretty = self.describe();
        let program = self.inner.as_std().get_program();
        let program = Path::new(program).file_stem().unwrap_or_default().to_os_string();
        let status_checker = self.status_checker.clone();
        debug!("Will run: {}", pretty);
        let child = self.inner.group_spawn();
        async move {
            let program = program.to_string_lossy();
            let mut child = child?.into_inner();
            // FIXME unwraps
            spawn_log_processor(format!("{program}:out"), child.stdout.take().unwrap());
            spawn_log_processor(format!("{program}:err"), child.stderr.take().unwrap());
            let status = child.wait().await?;
            status_checker(status).context(format!("Command failed: {}", pretty))
        }
        .boxed()
    }

    pub fn output_ok(&mut self) -> BoxFuture<'static, Result<Output>> {
        let pretty = self.describe();
        self.stdout(Stdio::piped());
        self.stderr(Stdio::piped());
        let child = self.spawn();
        let status_checker = self.status_checker.clone();
        async move {
            let child = child?;
            let output =
                child.wait_with_output().await.context("Failed while waiting for output.")?;
            status_checker(output.status).with_context(|| {
                format!(
                    "Stdout:\n{}\n\nStderr:\n{}\n",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr),
                )
            })?;
            Result::Ok(output)
        }
        .map_err(move |e| e.context(format!("Failed to get output of the command: {}", pretty)))
        .boxed()
    }

    pub fn run_stdout(&mut self) -> BoxFuture<'static, Result<String>> {
        let output = self.output_ok();
        async move {
            output
                .await?
                .into_stdout_string()
                .context("Failed to decode standard output as UTF8 text.")
        }
        .boxed()
    }

    pub fn spawn(&mut self) -> Result<Child> {
        let pretty = self.describe();
        debug!("Spawning {}", pretty);
        self.inner.spawn().context(format!("Failed to spawn: {}", pretty))
    }

    // pub fn status(&mut self) -> BoxFuture<'static, Result<ExitStatus>> {
    //     let fut = self.borrow_mut_command().status();
    //     async move { fut.await.anyhow_err() }.boxed()
    // }
    //
    // pub fn output(&mut self) -> BoxFuture<'static, Result<Output>> {
    //     let fut = self.borrow_mut_command().output();
    //     async move { fut.await.anyhow_err() }.boxed()
    // }
}

pub fn spawn_log_processor(
    prefix: String,
    out: impl AsyncRead + Send + Unpin + 'static,
) -> JoinHandle<Result> {
    tokio::task::spawn(async move {
        let bufread = BufReader::new(out);
        let mut lines = bufread.lines();
        while let Some(line) = lines.next_line().await? {
            debug!("{} {}", prefix, line)
        }
        debug!("{} {}", prefix, "<ENDUT>");
        Result::Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::global::new_spinner;
    // use crate::global::println;
    use tokio::io::AsyncBufReadExt;
    use tokio::io::AsyncRead;
    use tokio::io::BufReader;
    use tokio::process::ChildStdout;
    use tokio::task::JoinHandle;

    // pub fn spawn_log_processor(
    //     prefix: String,
    //     out: impl AsyncRead + Send + Unpin + 'static,
    // ) -> JoinHandle<Result> {
    //     tokio::task::spawn(async move {
    //         let bufread = BufReader::new(out);
    //         let mut lines = bufread.lines();
    //         while let Some(line) = lines.next_line().await? {
    //             println(format!("{} {}", prefix, line))
    //         }
    //         println(format!("{} {}", prefix, "<ENDUT>"));
    //         Result::Ok(())
    //     })
    // }U
    //
    // pub fn spawn_logged(cmd: &mut Command) {
    //     cmd.stdout(Stdio::piped());
    //     cmd.stderr(Stdio::piped());
    // }
    //
    // #[tokio::test]
    // async fn test_cmd_out_interception() -> Result {
    //     pretty_env_logger::init();
    //     let mut cmd = Command::new("cargo");
    //     cmd.arg("update");
    //     cmd.stdout(Stdio::piped());
    //     cmd.stderr(Stdio::piped());
    //
    //     let mut child = cmd.spawn()?;
    //     spawn_log_processor("[out]".into(), child.stdout.take().unwrap());
    //     spawn_log_processor("[err]".into(), child.stderr.take().unwrap());
    //     let bar = new_spinner(format!("Running {:?}", cmd));
    //     child.wait().await?;
    //     Ok(())
    // }
    //
    // #[tokio::test]
    // async fn spawning() -> Result {
    //     println!("Start");
    //     tokio::process::Command::new("python").spawn()?.wait().await?;
    //     println!("Finish");
    //     Ok(())
    // }
}
