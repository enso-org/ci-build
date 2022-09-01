//! See: https://docs.github.com/en/actions/learn-github-actions/environment-variables

use crate::env::Variable;
use crate::models::config::RepoContext;
use crate::prelude::*;
// use octocrab::models::RunId;


/// Always set to `true` when GitHub Actions is running the workflow. You can use this variable
/// to differentiate when tests are being run locally or by GitHub Actions.
///
/// See: <https://docs.github.com/en/actions/learn-github-actions/environment-variables#default-environment-variables>
#[derive(Clone, Copy, Debug)]
pub struct Actions;
impl Variable for Actions {
    const NAME: &'static str = "GITHUB_ACTIONS";
    type Value = bool;
}

#[derive(Clone, Copy, Debug)]
pub struct EnvFile;

impl Variable for EnvFile {
    const NAME: &'static str = "GITHUB_ENV";
    type Value = PathBuf;
}

/// The name of the runner executing the job.
#[derive(Clone, Copy, Debug)]
pub struct RunnerName;

impl Variable for RunnerName {
    const NAME: &'static str = "RUNNER_NAME";
}

/// Fails when called outside of GitHub Actions environment,
pub fn is_self_hosted() -> Result<bool> {
    let name = RunnerName.fetch()?;
    Ok(!name.starts_with("GitHub Actions"))
}

crate::define_env_var! {
    /// The owner and repository name. For example, `octocat/Hello-World`.
    GITHUB_REPOSITORY, RepoContext
}
crate::define_env_var! {
    /// The commit SHA that triggered the workflow. For example,
    /// `"ffac537e6cbbf934b08745a378932722df287a53"`.
    GITHUB_SHA, String
}
crate::define_env_var! {
    /// A unique number for each workflow run within a repository. This number does not change if you re-run the workflow run. For example, `1658821493`.
    GITHUB_RUN_ID, octocrab::models::RunId
}
