//! See: https://docs.github.com/en/actions/learn-github-actions/environment-variables

use crate::prelude::*;

pub const RUNNER_NAME_VAR: &str = "RUNNER_NAME";

/// The name of the runner executing the job.
pub fn runner_name() -> Result<String> {
    crate::env::expect_var(RUNNER_NAME_VAR)
}
