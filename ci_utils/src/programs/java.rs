use crate::prelude::*;

pub struct Java;

impl Program for Java {
    fn executable_name() -> &'static str {
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
