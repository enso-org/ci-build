use crate::prelude::*;
use std::borrow::BorrowMut;

pub mod command;
pub mod resolver;
pub mod shell;
pub mod version;
pub mod with_cwd;

use crate::program::command::MyCommand;
pub use resolver::Resolver;
pub use shell::Shell;

pub const EMPTY_ARGS: [&str; 0] = [];

/// A set of utilities for using a known external program.
///
/// The trait covers program lookup and process management.
// `Sized + 'static` bounds are due to using `Self` as type parameter for `Command` constructor.
#[async_trait]
pub trait Program: Sized + 'static {
    type Command: MyCommand + Send + Sync = Command;

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

    fn default_locations(&self) -> Vec<PathBuf> {
        Vec::new()
    }

    fn pretty_name() -> &'static str {
        Self::executable_name()
    }

    /// Locate the program executable.
    ///
    /// The lookup locations are program-defined, they typically include Path environment variable
    /// and program-specific default locations.
    fn lookup(&self) -> anyhow::Result<PathBuf> {
        Resolver::new(Self::executable_names(), self.default_locations())?.lookup()
    }

    async fn require_present(&self) -> Result<String> {
        let version = self.version_string().await?;
        println!("Found {}: {}", Self::executable_name(), version);
        Ok(version)
    }

    async fn require_present_at(&self, required_version: &Version) -> Result {
        let found_version = self.require_present().await?;
        let found_version = self.parse_version(&found_version)?;
        if &found_version != required_version {
            bail!(
                "Failed to find {} in version == {}. Found version: {}",
                Self::executable_name(),
                required_version,
                found_version
            )
        }
        Ok(())
    }

    fn cmd(&self) -> Result<Self::Command> {
        let program_path = self.lookup()?;
        let mut command = Self::Command::new_program::<Self, _>(program_path);
        if let Some(current_dir) = self.current_directory() {
            command.borrow_mut().current_dir(current_dir);
        }
        Ok(command)
    }

    fn current_directory(&self) -> Option<PathBuf> {
        None
    }

    fn handle_exit_status(status: std::process::ExitStatus) -> Result {
        status.exit_ok().anyhow_err()
    }

    /// Command that prints to stdout the version of given program.
    ///
    /// If this is anything other than `--version` the implementor should overwrite this method.
    fn version_command(&self) -> Result<Self::Command> {
        let mut cmd = self.cmd()?;
        cmd.borrow_mut().arg("--version");
        Ok(cmd)
    }

    async fn version_string(&self) -> Result<String> {
        let output = self.version_command()?.borrow_mut().output().await?;
        let string = String::from_utf8(output.stdout)?;
        Ok(string.trim().to_string())
    }

    // TODO if such need appears, likely Version should be made an associated type
    async fn version(&self) -> Result<Version> {
        let stdout = self.version_string().await?;
        self.parse_version(&stdout)
    }

    /// Retrieve semver-compatible version from the string in format provided by the
    /// `version_string`.
    ///
    /// Some programs do not follow semver for versioning, for them this method is unspecified.
    fn parse_version(&self, version_text: &str) -> Result<Version> {
        version::find_in_text(version_text)
    }
}

pub trait ProgramExt: Program {
    fn executable_names() -> Vec<&'static str> {
        let mut ret = vec![Self::executable_name()];
        ret.extend(Self::executable_name_fallback());
        ret
    }

    fn args(&self, args: impl IntoIterator<Item: AsRef<OsStr>>) -> Result<Self::Command> {
        let mut cmd = self.cmd()?;
        cmd.borrow_mut().args(args);
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
        cmd.borrow_mut().run_ok().boxed()
    }
}

impl<T> ProgramExt for T where T: Program {}
