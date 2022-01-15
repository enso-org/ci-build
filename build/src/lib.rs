use crate::prelude::*;

use regex::Regex;

pub mod prelude {
    pub use ide_ci::prelude::*;
}

pub mod bump_version;
pub mod httpbin;
pub mod paths;
pub mod postgres;
pub mod preflight_check;

pub fn get_enso_version(build_sbt_contents: &str) -> Result<Version> {
    let version_regex = Regex::new(r#"(?m)^val *ensoVersion *= *"([^"]*)".*$"#)?;
    let version_string = version_regex
        .captures(&build_sbt_contents)
        .ok_or_else(|| anyhow!("Failed to find line with version string."))?
        .get(1)
        .expect("Missing subcapture #1 with version despite matching the regex.")
        .as_str();
    Version::parse(version_string).anyhow_err()
}

#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    pub fn get_enso_version_test() -> Result {
        let contents = r#"
val scalacVersion  = "2.13.6"
val rustVersion    = "1.58.0-nightly"
val graalVersion   = "21.1.0"
val javaVersion    = "11"
val ensoVersion    = "0.2.32-SNAPSHOT"  // Note [Engine And Launcher Version]
val currentEdition = "2021.20-SNAPSHOT" // Note [Default Editions]
val stdLibVersion  = ensoVersion
"#;
        let version = get_enso_version(contents)?;
        assert_eq!(version.major, 0);
        assert_eq!(version.minor, 2);
        assert_eq!(version.patch, 32);
        assert_eq!(version.pre.as_str(), "SNAPSHOT");

        println!("{}\n{:?}", version, version);
        Ok(())
    }
}
