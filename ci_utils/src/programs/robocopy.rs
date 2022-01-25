/// Windows-specific system tool for copying things.
///
/// See https://docs.microsoft.com/en-us/windows-server/administration/windows-commands/robocopy
use crate::prelude::*;

pub struct Robocopy;

impl Program for Robocopy {
    fn executable_name() -> &'static str {
        "robocopy"
    }
}

impl Robocopy {
    // pub fn mirror_dir(
    //     source: impl AsRef<Path>,
    //     destination: impl AsRef<Path>,
    // ) -> anyhow::Result<()> {
    //     let mut cmd: Command = cmd_from_args![source.as_ref(), destination.as_ref(), "/mir"]?;
    //     let code = cmd.spawn()?.wait()?.code();
    //     if code == Some(1) {
    //         Ok(())
    //     } else {
    //         Err(Error::Blah.into())
    //     }
    // }
}
//
// #[derive(Clone, Copy, Debug, Error)]
// pub enum Error {
//     #[error("ROBOCOPY failed")]
//     Blah,
// }
