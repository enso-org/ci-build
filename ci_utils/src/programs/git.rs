use crate::new_command_type;
use crate::prelude::*;

use crate::program::command::Manipulator;

#[derive(Clone, Debug, Default)]
pub struct Git {
    pub repo_path: Option<PathBuf>,
}

impl Program for Git {
    type Command = GitCommand;
    fn executable_name() -> &'static str {
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
    pub fn nice_clean(&mut self) -> &mut Self {
        self.arg(Command::Clean)
            .apply(&Clean::Ignored)
            .apply(&Clean::Force)
            .apply(&Clean::UntrackedDirectories)
            .apply(&Clean::Exclude(".idea".into()))
            .apply(&Clean::Exclude(
                PathBuf::from_iter(["target", "enso-build"]).display().to_string(),
            ))
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

pub enum Clean {
    /// Normally, when no path is specified, `git clean` will not recurse into untracked
    /// directories to avoid removing too much. Specify this option to have it recurse into such
    /// directories as well. If any paths are specified, this option is irrelevant; all untracked
    /// files matching the specified paths (with exceptions for nested git directories mentioned
    /// under `Force`) will be removed.
    UntrackedDirectories,

    /// If the Git configuration variable clean.requireForce is not set to false, git clean will
    /// refuse to delete files or directories unless given `Force` or `Interactive`. Git will
    /// refuse to modify untracked nested git repositories (directories with a .git subdirectory)
    /// unless a second `Force` is given.
    Force,

    /// Show what would be done and clean files interactively.
    Interactive,

    /// Don’t actually remove anything, just show what would be done.
    DryRun,

    /// Use the given exclude pattern in addition to the standard ignore rules.
    Exclude(String),

    /// Don’t use the standard ignore rules, but still use the ignore rules given with `Exclude`
    /// options from the command line. This allows removing all untracked files, including build
    /// products. This can be used (possibly in conjunction with git restore or git reset) to
    /// create a pristine working directory to test a clean build.
    Ignored,

    /// Remove only files ignored by Git. This may be useful to rebuild everything from scratch,
    /// but keep manually created files.
    OnlyIgnored,
}

impl Manipulator for Clean {
    fn apply<C: IsCommandWrapper + ?Sized>(&self, command: &mut C) {
        // fn apply<'a, C: IsCommandWrapper + ?Sized>(&self, c: &'a mut C) -> &'a mut C {
        let args: Vec<&str> = match self {
            Clean::UntrackedDirectories => vec!["-d"],
            Clean::Force => vec!["-f"],
            Clean::Interactive => vec!["-i"],
            Clean::DryRun => vec!["-n"],
            Clean::Exclude(pattern) => vec!["-e", pattern.as_ref()],
            Clean::Ignored => vec!["-x"],
            Clean::OnlyIgnored => vec!["-X"],
        };
        command.args(args);
    }
}
