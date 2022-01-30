use crate::prelude::*;
use regex::Regex;

// Taken from the official semver description:
// https://semver.org/#is-there-a-suggested-regular-expression-regex-to-check-a-semver-string
const SEMVER_REGEX_CODE: &str = r"(?P<major>0|[1-9]\d*)\.(?P<minor>0|[1-9]\d*)\.(?P<patch>0|[1-9]\d*)(?:-(?P<prerelease>(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)(?:\.(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*))?(?:\+(?P<buildmetadata>[0-9a-zA-Z-]+(?:\.[0-9a-zA-Z-]+)*))?";

/// Regular expression that matches a semver within a text.
pub fn semver_regex() -> Regex {
    // unwrap safe, as this is covered by test `semver_regex_parses`.
    Regex::new(SEMVER_REGEX_CODE).unwrap()
}

pub fn find_in_text(text: &str) -> anyhow::Result<Version> {
    let regex = semver_regex();
    let matched = regex
        .find(text)
        .ok_or_else(|| anyhow!("Failed to find semver string within the following: {}", text))?;
    let version_text = matched.as_str();
    Version::parse(version_text).map_err(<_>::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_regex_parses() {
        semver_regex(); // Does not panic.
    }

    #[test]
    fn parse_cargo() -> Result {
        let text = "cargo 1.57.0-nightly (c7957a74b 2021-10-11)";
        let version = find_in_text(text)?;
        assert_eq!(version.major, 1);
        assert_eq!(version.minor, 57);
        assert_eq!(version.patch, 0);
        assert_eq!(version.pre, semver::Prerelease::new("nightly")?);
        assert_eq!(version.build, <_>::default());
        Ok(())
    }
}
