use crate::prelude::*;

pub mod resolver;
pub mod shell;
pub mod version;
pub mod with_cwd;

pub use resolver::Resolver;
pub use shell::Shell;

/// A set of utilities for using a known external program.
///
/// The trait covers program lookup and process management.
#[async_trait]
pub trait Program {
    /// The name used to find and invoke the program.
    ///
    /// This should just the stem name, not a full path. The os-specific executable extension should
    /// be skipped.
    fn executable_name() -> &'static str;

    /// If program can be found under more than one name, additional names are provided.
    ///
    /// The primary name is provided by ['executable_name'].
    fn executable_name_fallback() -> Vec<&'static str> {
        vec![]
    }

    fn executable_names() -> Vec<&'static str> {
        let mut ret = vec![Self::executable_name()];
        ret.extend(Self::executable_name_fallback());
        ret
    }

    fn default_locations(&self) -> Vec<PathBuf> {
        Vec::new()
    }

    fn pretty_name() -> &'static str {
        Self::executable_name()
    }

    fn lookup_all(&self) -> Result<Box<dyn Iterator<Item = PathBuf>>> {
        Ok(Box::new(
            Resolver::new(Self::executable_names(), self.default_locations())?.lookup_all(),
        ))
    }

    /// Locate the program executable.
    ///
    /// The lookup locations are program-defined, they typically include Path environment variable
    /// and program-specific default locations.
    fn lookup(&self) -> anyhow::Result<PathBuf> {
        Resolver::new(Self::executable_names(), self.default_locations())?.lookup()
    }

    async fn require_present(&self) -> Result {
        println!("Found {}: {}", Self::executable_name(), self.version_string().await?.trim());
        Ok(())
    }

    fn cmd(&self) -> Result<Command> {
        let mut command = self.lookup().map(Command::new)?;
        if let Some(current_dir) = self.current_directory() {
            command.current_dir(current_dir);
        }
        Ok(command)
    }

    fn current_directory(&self) -> Option<PathBuf> {
        None
    }

    async fn wait(mut command: Command) -> Result {
        let status = command.status().await?;
        Self::handle_exit_status(status)
    }

    fn handle_exit_status(status: std::process::ExitStatus) -> anyhow::Result<()> {
        status.exit_ok().anyhow_err()
    }

    fn version_command(&self) -> Result<Command> {
        let mut cmd = self.cmd()?;
        cmd.arg("--version");
        Ok(cmd)
    }

    async fn version_string(&self) -> Result<String> {
        let output = self.version_command()?.output().await?;
        String::from_utf8(output.stdout).anyhow_err()
    }

    async fn version(&self) -> Result<Version> {
        let stdout = self.version_string().await?;
        version::find_in_text(&stdout)
    }
}

pub trait ProgramExt: Program {
    fn args(&self, args: impl IntoIterator<Item: AsRef<OsStr>>) -> Result<Command> {
        let mut cmd = self.cmd()?;
        cmd.args(args);
        Ok(cmd)
    }

    fn call_arg(&self, arg: impl AsRef<OsStr>) -> BoxFuture<'static, Result> {
        self.call_args(once(arg))
    }

    // We cannot use async_trait for this, as we need to separate lifetime of the future from the
    // arguments' lifetimes.
    fn call_args(&self, args: impl IntoIterator<Item: AsRef<OsStr>>) -> BoxFuture<'static, Result> {
        let mut cmd = match self.args(args) {
            Ok(cmd) => cmd,
            e @ Err(_) => return ready(e.map(|_| ())).boxed(),
        };
        cmd.run_ok().boxed()
    }
}

impl<T> ProgramExt for T where T: Program {}
