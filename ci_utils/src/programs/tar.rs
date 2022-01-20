use crate::prelude::*;

use crate::archive::Format;


#[derive(Clone, Copy, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub enum Compression {
    Bzip2,
    Gzip,
    Lzma,
    Xz,
}

impl Compression {
    pub fn deduce_from_extension(extension: impl AsRef<Path>) -> Result<Compression> {
        let extension = extension.as_ref().to_str().unwrap();
        if extension == "bz2" {
            Ok(Compression::Bzip2)
        } else if extension == "gz" {
            Ok(Compression::Gzip)
        } else if extension == "lzma" {
            Ok(Compression::Lzma)
        } else if extension == "xz" {
            Ok(Compression::Xz)
        } else {
            bail!("The extension `{}` does not denote a supported compression algorithm for TAR archives.", extension)
        }
    }
}

impl Display for Compression {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use Compression::*;
        write!(f, "{}", match self {
            Bzip2 => "bzip2",
            Gzip => "gzip",
            Lzma => "lzma",
            Xz => "xz",
        })
    }
}

impl Compression {
    pub fn format_argument(&self) -> &str {
        match self {
            Compression::Bzip2 => "-j",
            Compression::Gzip => "-z",
            Compression::Lzma => "--lzma",
            Compression::Xz => "-J",
        }
    }
}

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq)]
pub enum Switch {
    TargetFile(PathBuf),
    Verbose,
    UseFormat(Compression),
}

impl Switch {
    fn format_arguments(&self) -> Vec<String> {
        match self {
            Switch::TargetFile(tgt) => vec!["-f".into(), tgt.to_string_lossy().into()],
            Switch::Verbose => vec!["--verbose".into()],
            Switch::UseFormat(compression) => vec![compression.format_argument().into()],
        }
    }
}

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq)]
pub enum Command {
    Append,
    Create,
    Extract,
    List,
}

impl Command {
    fn format_argument(&self) -> &str {
        match self {
            Command::Append => "-r",
            Command::Create => "-c",
            Command::Extract => "-x",
            Command::List => "-t",
        }
    }
}

pub struct Tar;

impl Program for Tar {
    fn executable_name() -> &'static str {
        "tar"
    }
}

impl Tar {
    pub fn pack_cmd<P: AsRef<Path>>(
        &self,
        output_archive: impl AsRef<Path>,
        paths_to_pack: impl IntoIterator<Item = P>,
    ) -> Result<crate::prelude::Command> {
        let mut cmd = self.cmd()?;
        cmd.arg(Command::Create.format_argument());

        if let Ok(Format::Tar(Some(compression))) = Format::from_filename(&output_archive) {
            cmd.args(Switch::UseFormat(compression).format_arguments());
        }

        cmd.arg(output_archive.as_ref());
        for path_to_pack in paths_to_pack {
            cmd.arg(path_to_pack.as_ref());
        }
        Ok(cmd)
        // cmd_from_args![Command::Create, val [switches], output_archive.as_ref(), ref
        // [paths_to_pack]]
    }

    pub async fn pack<P: AsRef<Path>>(
        self,
        output_archive: impl AsRef<Path>,
        paths_to_pack: impl IntoIterator<Item = P>,
    ) -> Result {
        self.pack_cmd(output_archive, paths_to_pack)?.run_ok().await
    }
}


#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn deduce_format_from_extension() {
        let expect_ok = |str: &str, expected: Compression| {
            assert_eq!(Compression::deduce_from_extension(&OsStr::new(str)).unwrap(), expected);
        };

        expect_ok("bz2", Compression::Bzip2);
        expect_ok("gz", Compression::Gzip);
        expect_ok("lzma", Compression::Lzma);
        expect_ok("xz", Compression::Xz);
    }

    #[test]
    fn pack_command_test() {
        let cmd = Tar.pack_cmd("output.tar.gz", &["target.bmp"]).unwrap();
        println!("{:?}", cmd);
        dbg!(cmd);
    }
}
