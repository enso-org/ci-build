/// Windows-specific system tool for copying things.
///
/// See https://docs.microsoft.com/en-us/windows-server/administration/windows-commands/robocopy
use crate::prelude::*;

pub struct Robocopy;

impl Program for Robocopy {
    fn executable_name() -> &'static str {
        "robocopy"
    }

    fn handle_exit_status(status: std::process::ExitStatus) -> Result {
        match status.code() {
            None => status.exit_ok().anyhow_err(),
            Some(code) if code >= 8 => bail!("Exit with code {}.", code),
            Some(_) => Ok(()),
        }
    }
}

impl Robocopy {}

pub async fn mirror_directory(source: impl AsRef<Path>, destination: impl AsRef<Path>) -> Result {
    Robocopy
        .cmd()?
        .arg(source.as_ref())
        .arg(destination.as_ref())
        .arg("/mir")
        .arg("/sl")
        .run_ok()
        .await
}
