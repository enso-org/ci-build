//! See: https://docs.github.com/en/actions/learn-github-actions/environment-variables

use crate::models::config::RepoContext;
use crate::prelude::*;

pub const RUNNER_NAME_VAR: &str = "RUNNER_NAME";

/// The name of the runner executing the job.
pub fn runner_name() -> Result<String> {
    crate::env::expect_var(RUNNER_NAME_VAR)
}

pub fn is_self_hosted() -> bool {
    if let Ok(name) = runner_name() {
        name.starts_with("GitHub Actions")
    } else {
        false
    }
}

pub fn repository() -> Result<RepoContext> {
    let var_name = "GITHUB_REPOSITORY";
    let repo = crate::env::expect_var(var_name)?; // e.g. "octocat/Hello-World"
    match repo.split("/").collect_vec().as_slice() {
        [owner, name] => Ok(RepoContext { owner: owner.to_string(), name: name.to_string() }),
        _ => bail!(
            "Variable {} is present (value is `{}`) but does not match the expected format.",
            var_name,
            repo
        ),
    }
}
