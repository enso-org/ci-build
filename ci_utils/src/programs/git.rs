use crate::prelude::*;

use crate::new_command_type;

pub mod clean;

pub use clean::Clean;

#[derive(Clone, Debug, Default)]
pub struct Git {
    pub repo_path: Option<PathBuf>,
}

impl Program for Git {
    type Command = GitCommand;
    fn executable_name(&self) -> &'static str {
        "git"
    }
    fn current_directory(&self) -> Option<PathBuf> {
        self.repo_path.clone()
    }
}

impl Git {
    pub fn new(repo_path: impl Into<PathBuf>) -> Self {
        // TODO likely should normalize path to repo root (from e.g. repo subtree path)
        //      but consider e.g. being invoked in the submodule tree
        Self { repo_path: Some(repo_path.into()) }
    }

    pub async fn head_hash(&self) -> Result<String> {
        self.cmd()?.args(["rev-parse", "--verify", "HEAD"]).output_ok().await?.single_line_stdout()
    }
}


new_command_type!(Git, GitCommand);

impl GitCommand {
    pub fn clean(&mut self) -> &mut Self {
        self.arg(Command::Clean)
            .apply(&Clean::Ignored)
            .apply(&Clean::Force)
            .apply(&Clean::UntrackedDirectories)
    }
    pub fn nice_clean(&mut self) -> &mut Self {
        self.clean().apply(&Clean::Exclude(".idea".into()))
    }
}

pub enum Command {
    Clean,
}

impl AsRef<OsStr> for Command {
    fn as_ref(&self) -> &OsStr {
        match self {
            Command::Clean => OsStr::new("clean"),
        }
    }
}
