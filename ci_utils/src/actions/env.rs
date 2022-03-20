//! See: https://docs.github.com/en/actions/learn-github-actions/environment-variables

use crate::env::Variable;
use crate::models::config::RepoContext;
use crate::prelude::*;
// use octocrab::models::RunId;


/// Always set to `true` when GitHub Actions is running the workflow. You can use this variable
/// to differentiate when tests are being run locally or by GitHub Actions.
///
/// See: <https://docs.github.com/en/actions/learn-github-actions/environment-variables#default-environment-variables>
pub struct Actions;
impl Variable for Actions {
    const NAME: &'static str = "GITHUB_ACTIONS";
    type Value = bool;
}

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

/// Fails when called outside of GitHub Actions environment,
pub fn is_self_hosted() -> Result<bool> {
    let name = RunnerName.fetch()?;
    Ok(!name.starts_with("GitHub Actions"))
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
