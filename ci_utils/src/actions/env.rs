//! See: https://docs.github.com/en/actions/learn-github-actions/environment-variables

use crate::env::Variable;
use crate::models::config::RepoContext;
use crate::prelude::*;
// use octocrab::models::RunId;

pub struct EnvFile;

impl Variable for EnvFile {
    const NAME: &'static str = "GITHUB_ENV";
    type Value = PathBuf;
}

/// The name of the runner executing the job.
pub struct RunnerName;

impl Variable for RunnerName {
    const NAME: &'static str = "RUNNER_NAME";
}

pub fn is_self_hosted() -> bool {
    if let Ok(name) = RunnerName.fetch() {
        !name.starts_with("GitHub Actions")
    } else {
        false
    }
}

pub struct Repository;

impl Variable for Repository {
    const NAME: &'static str = "GITHUB_REPOSITORY";
    type Value = RepoContext;
}


pub struct Sha;

impl Variable for Sha {
    const NAME: &'static str = "GITHUB_SHA";
}

pub struct RunId;

impl Variable for RunId {
    const NAME: &'static str = "GITHUB_RUN_ID";
    type Value = octocrab::models::RunId;
}
