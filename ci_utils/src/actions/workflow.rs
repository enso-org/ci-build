use crate::prelude::*;
use std::io::Write;

/// Sets an action's output parameter.
///
/// See: <https://docs.github.com/en/actions/learn-github-actions/workflow-commands-for-github-actions#setting-an-output-parameter>
pub fn set_output(name: &str, value: &str) {
    iprintln!("::set-output name={name}::{value}");
}

/// Prints a debug message to the log.
///
/// You must create a secret named `ACTIONS_STEP_DEBUG` with the value `true` to see the debug
/// messages set by this command in the log.
///
/// See: <https://docs.github.com/en/actions/learn-github-actions/workflow-commands-for-github-actions#setting-a-debug-message>
pub fn debug(message: &str) {
    iprintln!("::debug::{message}")
}

/// Creates or updates an environment variable for any steps running next in a job.
///
/// This step and all subsequent steps in a job will have access to the variable. Environment
/// variables are case-sensitive and you can include punctuation.
pub fn set_env(name: &str, value: impl AsRef<str>) -> Result {
    let value = value.as_ref();
    iprintln!("Will try writing Github Actions environment variable: {name}={value}");
    std::env::set_var(name, value);
    let env_file = crate::env::expect_var_os("GITHUB_ENV")?;
    let line = iformat!("{name}={value}\n");
    std::fs::OpenOptions::new()
        .create_new(false)
        .append(true)
        .open(env_file)?
        .write_all(line.as_bytes())?;
    Ok(())
}

pub fn mask_text(text: impl AsRef<str>) {
    if std::env::var("GITHUB_ACTIONS").is_ok() {
        iprintln!("::add-mask::{text.as_ref()}")
    }
}

pub fn mask_environment_variable(variable_name: impl AsRef<str>) {
    if std::env::var("GITHUB_ACTIONS").is_ok() {
        iprintln!("::add-mask::${variable_name.as_ref()}")
    }
}
