use crate::prelude::*;

use crate::program::command::Manipulator;

#[derive(Clone, Debug, derive_more::Deref, derive_more::DerefMut)]
pub struct Classpath(pub Vec<PathBuf>);

impl Classpath {
    pub fn new(paths: impl IntoIterator<Item: AsRef<Path>>) -> Self {
        Classpath(paths.into_iter().map(|p| p.as_ref().to_path_buf()).collect())
    }
}

impl Manipulator for Classpath {
    fn apply<C: IsCommandWrapper + ?Sized>(&self, command: &mut C) {
        // Java uses same separator for classpaths entries as native PATH separator.
        let paths = std::env::join_paths(&self.0)
            .expect(&format!("Invalid character in paths: {:?}", &self.0));
        command.arg("--class-path").arg(paths);
    }
}

#[derive(Clone, Debug)]
pub enum Option {
    EnableAssertions,
}

impl AsRef<str> for Option {
    fn as_ref(&self) -> &str {
        match self {
            Option::EnableAssertions => "-enableassertions",
        }
    }
}

impl AsRef<OsStr> for Option {
    fn as_ref(&self) -> &OsStr {
        OsStr::new::<str>(self.as_ref())
    }
}

pub struct Java;

impl Program for Java {
    fn executable_name(&self) -> &'static str {
        "java"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_version() {
        let contents = "openjdk 11.0.11 2021-04-20\nOpenJDK Runtime Environment GraalVM CE 21.1.0 (build 11.0.11+8-jvmci-21.1-b05)\nOpenJDK 64-Bit Server VM GraalVM CE 21.1.0 (build 11.0.11+8-jvmci-21.1-b05, mixed mode, sharing)";
        assert_eq!(Java.parse_version(contents).unwrap(), Version::new(21, 1, 0));
    }
}

#[derive(Clone, Copy, Debug, Shrinkwrap)]
pub struct LanguageVersion(pub u8);

impl std::str::FromStr for LanguageVersion {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        s.parse2::<u8>().map(LanguageVersion)
    }
}

impl Display for LanguageVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "java{}", self.0)
    }
}
