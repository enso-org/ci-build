use crate::prelude::*;
use std::io::Write;

/// Sets an action's output parameter.
///
/// See: <https://docs.github.com/en/actions/learn-github-actions/workflow-commands-for-github-actions#setting-an-output-parameter>
pub fn set_output(name: &str, value: impl Display) {
    iprintln!("Setting GitHub Actions step output {name} to {value}");
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
///
/// Does nothing (except for log) when not being run under CI. Fails if used under non-GH CI.
pub fn set_env(name: &str, value: impl Display) -> Result {
    iprintln!("Will try writing Github Actions environment variable: {name}={value}");
    std::env::set_var(name, value.to_string());
    if std::env::var("CI").is_ok() {
        let env_file = crate::env::expect_var_os("GITHUB_ENV")?;
        let line = iformat!("{name}={value}\n");
        std::fs::OpenOptions::new()
            .create_new(false)
            .append(true)
            .open(env_file)?
            .write_all(line.as_bytes())?;
    }
    Ok(())
}

pub fn mask_text(text: impl AsRef<str>) {
    if std::env::var("GITHUB_ACTIONS").is_ok() {
        iprintln!("::add-mask::{text.as_ref()}")
    }
}

pub fn mask_value(value: impl Display) {
    if std::env::var("GITHUB_ACTIONS").is_ok() {
        iprintln!("::add-mask::{value}")
    }
}

pub fn mask_environment_variable(variable_name: impl AsRef<OsStr>) -> Result {
    mask_value(std::env::var(variable_name)?);
    Ok(())
}

#[derive(Clone, Copy, Debug, strum::Display)]
#[strum(serialize_all = "snake_case")]
pub enum MessageLevel {
    Debug,
    Notice,
    Warning,
    Error,
}

pub struct Message {
    pub level: MessageLevel,
    pub text:  String,
    // TODO title, line, column
}

impl Message {
    pub fn notice(text: impl AsRef<str>) {
        Message { level: MessageLevel::Notice, text: text.as_ref().into() }.send()
    }

    pub fn send(&self) {
        println!("::{} ::{}", self.level, self.text);
    }
}
