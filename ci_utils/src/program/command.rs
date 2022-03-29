use crate::prelude::*;
use anyhow::Context;
use std::borrow::BorrowMut;
use std::fmt::Debug;
use std::fmt::Formatter;
use std::ops::Deref;
use std::ops::DerefMut;
use std::process::ExitStatus;
use std::process::Output;
use std::process::Stdio;
use tokio::process::Child;

pub trait MyCommand: BorrowMut<Command> + From<Command> {
    fn new_program<P: Program + 'static, S: AsRef<OsStr>>(program: S) -> Self {
        let inner = Command::new_program::<P, S>(program);
        Self::from(inner)
    }
}

impl MyCommand for Command {
    fn new_program<P: Program + 'static, S: AsRef<OsStr>>(program: S) -> Self {
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

impl Deref for Command {
    type Target = tokio::process::Command;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Command {
    fn deref_mut(&mut self) -> &mut Self::Target {
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

    pub fn exit_ok(&self, status: std::process::ExitStatus) -> Result {
        (self.status_checker)(status)
    }

    pub fn run_ok(&mut self) -> BoxFuture<'static, Result<()>> {
        let pretty = self.describe();
        debug!("Will run: {}", pretty);
        let status = self.status();
        let status_checker = self.status_checker.clone();
        async move {
            let status = status.await?;
            status_checker(status).context(format!("Command failed: {}", pretty))
        }
        .boxed()
    }

    // FIXME check exit code
    pub fn output_ok(&mut self) -> BoxFuture<'static, Result<Output>> {
        let pretty = self.describe();
        debug!("Will run: {}", pretty);
        let output = self.output();
        async move { output.await.context(format!("Command failed: {}", pretty)) }.boxed()
    }

    pub fn spawn(&mut self) -> Result<Child> {
        let pretty = self.describe();
        debug!("Spawning {}", pretty);
        self.inner.spawn().context(format!("Failed to spawn: {}", pretty))
    }
}

impl Command {
    pub fn arg<S: AsRef<OsStr>>(&mut self, arg: S) -> &mut Command {
        self.inner.arg(arg);
        self
    }

    pub fn args<I, S>(&mut self, args: I) -> &mut Command
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>, {
        self.inner.args(args);
        self
    }

    pub fn env<K, V>(&mut self, key: K, val: V) -> &mut Command
    where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>, {
        self.inner.env(key, val);
        self
    }

    pub fn envs<I, K, V>(&mut self, vars: I) -> &mut Command
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>, {
        self.inner.envs(vars);
        self
    }

    pub fn env_remove<K: AsRef<OsStr>>(&mut self, key: K) -> &mut Command {
        self.inner.env_remove(key);
        self
    }

    pub fn env_clear(&mut self) -> &mut Command {
        self.inner.env_clear();
        self
    }

    pub fn current_dir<P: AsRef<Path>>(&mut self, dir: P) -> &mut Command {
        self.inner.current_dir(dir);
        self
    }

    pub fn stdin<T: Into<Stdio>>(&mut self, cfg: T) -> &mut Command {
        self.inner.stdin(cfg);
        self
    }

    pub fn stdout<T: Into<Stdio>>(&mut self, cfg: T) -> &mut Command {
        self.inner.stdout(cfg);
        self
    }

    pub fn stderr<T: Into<Stdio>>(&mut self, cfg: T) -> &mut Command {
        self.inner.stderr(cfg);
        self
    }

    #[cfg(windows)]
    #[cfg_attr(docsrs, doc(cfg(windows)))]
    pub fn creation_flags(&mut self, flags: u32) -> &mut Command {
        self.inner.creation_flags(flags);
        self
    }

    #[cfg(unix)]
    #[cfg_attr(docsrs, doc(cfg(unix)))]
    pub fn uid(&mut self, id: u32) -> &mut Command {
        self.inner.uid(id);
        self
    }

    #[cfg(unix)]
    #[cfg_attr(docsrs, doc(cfg(unix)))]
    pub fn gid(&mut self, id: u32) -> &mut Command {
        self.inner.gid(id);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncBufReadExt;
    use tokio::io::BufReader;

    #[tokio::test]
    async fn aabhgf() -> Result {
        pretty_env_logger::init();
        let mut cmd = Command::new("cargo");
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        let mut child = cmd.spawn()?;
        let out = child.stdout.take().unwrap();
        let err = child.stderr.take().unwrap();


        tokio::task::spawn(async move {
            let bufread = BufReader::new(out);
            let mut lines = bufread.lines();
            while let Some(line) = lines.next_line().await? {
                debug!("[cargo out] {}", line)
            }
            debug!("[cargo out] <ENDUT>");
            Result::Ok(())
        });

        tokio::task::spawn(async move {
            let bufread = BufReader::new(err);
            let mut lines = bufread.lines();
            while let Some(line) = lines.next_line().await? {
                debug!("[cargo err] {}", line)
            }
            debug!("[cargo err] <ENDUT>");
            Result::Ok(())
        });

        child.wait().await?;
        Ok(())
    }
}
