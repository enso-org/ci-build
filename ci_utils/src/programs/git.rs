use crate::prelude::*;

#[derive(Clone, Debug, Default)]
pub struct Git {
    pub repo_path: Option<PathBuf>,
}

impl Program for Git {
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

    pub async fn clean_xfd(&self) -> Result {
        self.cmd()?.arg("clean").arg("-xfd").args(["-e", ".idea/"]).run_ok().await
    }
}
