use crate::prelude::*;
use snafu::Snafu;

pub struct SevenZip;

impl Program for SevenZip {
    fn executable_name() -> &'static str {
        "7z"
    }
    fn executable_name_fallback() -> Vec<&'static str> {
        vec!["7za"]
    }

    fn default_locations(&self) -> Vec<PathBuf> {
        if let Ok(program_files) = std::env::var("ProgramFiles") {
            let path = PathBuf::from(program_files).join("7-Zip");
            if path.exists() {
                return vec![path];
            }
        }
        vec![]
    }

    fn handle_exit_status(status: std::process::ExitStatus) -> anyhow::Result<()> {
        if status.success() {
            Ok(())
        } else if let Some(code) = status.code() {
            Err(ExecutionError::from_exit_code(code).into())
        } else {
            Err(ExecutionError::Unknown.into())
        }
    }
}

// Cf https://7zip.bugaco.com/7zip/MANUAL/cmdline/exit_codes.htm
#[derive(Snafu, Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub enum ExecutionError {
    #[snafu(display(
        "Warning (Non fatal error(s)). For example, one or more files were locked by some \
    other application, so they were not compressed."
    ))]
    Warning,
    #[snafu(display("Fatal error"))]
    Fatal,
    #[snafu(display("Command line error"))]
    CommandLine,
    #[snafu(display("Not enough memory for operation"))]
    NotEnoughMemory,
    #[snafu(display("User stopped the process"))]
    UserStopped,
    #[snafu(display("Unrecognized error code"))]
    Unknown,
}

impl ExecutionError {
    fn from_exit_code(code: i32) -> Self {
        match code {
            1 => Self::Warning,
            2 => Self::Fatal,
            7 => Self::CommandLine,
            8 => Self::NotEnoughMemory,
            255 => Self::UserStopped,
            _ => Self::Unknown,
        }
    }
}

impl SevenZip {
    pub fn pack_cmd<P: AsRef<OsStr>>(
        &self,
        output_archive: impl AsRef<OsStr>,
        paths_to_pack: impl IntoIterator<Item = P>,
    ) -> Result<Command> {
        let output_archive = output_archive.as_ref();
        let mut cmd = self.cmd()?;
        cmd.arg(ArchiveCommand::Add)
            .args(Switch::AssumeYes)
            .arg(output_archive)
            .args(paths_to_pack);
        Ok(cmd)
    }

    pub async fn pack<P: AsRef<OsStr>>(
        &self,
        output_archive: impl AsRef<OsStr>,
        paths_to_pack: impl IntoIterator<Item = P>,
    ) -> Result {
        crate::io::remove_if_exists(output_archive.as_ref())?;
        self.pack_cmd(output_archive, paths_to_pack)?.run_ok().await
    }

    pub fn unpack_cmd(
        &self,
        archive: impl AsRef<OsStr>,
        output_directory: impl AsRef<OsStr>,
    ) -> Result<Command> {
        let out_switch = Switch::OutputDirectory(output_directory.as_ref().into());
        let mut cmd = self.cmd()?;
        cmd.arg(ArchiveCommand::ExtractWithFullPaths)
            .args(Switch::AssumeYes)
            .args(out_switch)
            .arg(archive);
        Ok(cmd)
    }
}

#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub enum ArchiveCommand {
    Add,
    ExtractWithFullPaths,
}

impl AsRef<OsStr> for ArchiveCommand {
    fn as_ref(&self) -> &OsStr {
        match self {
            Self::Add => "a",
            Self::ExtractWithFullPaths => "x",
        }
        .as_ref()
    }
}

// https://sevenzip.osdn.jp/chm/cmdline/switches/index.htm
#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub enum Switch {
    OutputDirectory(PathBuf),
    AssumeYes,
    OverwriteMode(OverwriteMode),
    RedirectStream(StreamType, StreamDestination),
    SetCharset(Charset),
}

#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub enum OverwriteMode {
    OverwriteAll,
    SkipExisting,
    AutoRenameExtracted,
    AutoRenameExisting,
}

#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub enum StreamType {
    StandardOutput,
    ErrorOutput,
    ProgressInformation,
}

impl From<StreamType> for OsString {
    fn from(value: StreamType) -> Self {
        match value {
            StreamType::StandardOutput => "o",
            StreamType::ErrorOutput => "e",
            StreamType::ProgressInformation => "p",
        }
        .into()
    }
}

#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub enum StreamDestination {
    DisableStream,
    RedirectToStdout,
    RedirectToStderr,
}

impl From<StreamDestination> for OsString {
    fn from(value: StreamDestination) -> Self {
        match value {
            StreamDestination::DisableStream => "0",
            StreamDestination::RedirectToStdout => "1",
            StreamDestination::RedirectToStderr => "2",
        }
        .into()
    }
}

#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub enum Charset {
    Utf8,
    Win,
    Dos,
}

impl From<Charset> for OsString {
    fn from(value: Charset) -> Self {
        match value {
            Charset::Utf8 => "UTF-8",
            Charset::Win => "WIN",
            Charset::Dos => "DOS",
        }
        .into()
    }
}

impl IntoIterator for Switch {
    type Item = OsString;
    type IntoIter = std::vec::IntoIter<OsString>;

    fn into_iter(self) -> Self::IntoIter {
        use OverwriteMode::*;
        match self {
            Self::OutputDirectory(dir) => vec!["-o".into(), dir.into()],
            Self::AssumeYes => vec!["-y".into()],
            Self::OverwriteMode(OverwriteAll) => vec!["-aoa".into()],
            Self::OverwriteMode(SkipExisting) => vec!["-aos".into()],
            Self::OverwriteMode(AutoRenameExtracted) => vec!["-aou".into()],
            Self::OverwriteMode(AutoRenameExisting) => vec!["-aot".into()],
            Self::RedirectStream(str, dest) => vec!["-bs".into(), str.into(), dest.into()],
            Self::SetCharset(charset) => vec!["-scc".into(), charset.into()],
        }
        .into_iter()
    }
}
