// use crate::preflight_check::NIGHTLY_RELEASE_TITLE_INFIX;
use crate::prelude::*;
use chrono::Datelike;
use ide_ci::models::config::RepoContext;
use octocrab::models::repos::Release;
use semver::Prerelease;
use std::collections::BTreeSet;
use std::fmt::Formatter;
use std::str::FromStr;

/// Variable that stores Enso Engine version.
const VERSION_VAR_NAME: &str = "ENSO_VERSION";
const EDITION_VAR_NAME: &str = "ENSO_EDITION";
const RELEASE_MODE_VAR_NAME: &str = "ENSO_RELEASE_MODE";

const LOCAL_BUILD_PREFIX: &str = "dev";
const NIGHTLY_BUILD_PREFIX: &str = "nightly";
// pub enum Kind {
//     Local,
//     Nightly,
//     // Rc,
//     // Stable,
// }

pub fn default_engine_version() -> Version {
    let mut ret = Version::new(0, 0, 0);
    ret.pre = Prerelease::new(LOCAL_BUILD_PREFIX).unwrap();
    ret
}

pub fn is_nightly(release: &Release) -> bool {
    !release.draft && release.tag_name.contains(NIGHTLY_BUILD_PREFIX)
}

#[derive(Clone, Debug, Serialize, Deserialize, Shrinkwrap)]
pub struct Versions {
    pub version:      Version,
    #[shrinkwrap(main_field)]
    pub release_mode: bool,
}

impl Default for Versions {
    fn default() -> Self {
        Versions { version: default_engine_version(), release_mode: false }
    }
}

impl Versions {
    pub fn new(version: Version) -> Self {
        let release_mode = !version.pre.as_str().contains(LOCAL_BUILD_PREFIX);
        Versions { version, release_mode }
    }

    pub fn edition_name(&self) -> String {
        self.version.to_string()
    }

    pub fn local_prerelease() -> Result<Prerelease> {
        Prerelease::new(LOCAL_BUILD_PREFIX).anyhow_err()
    }

    pub async fn nightly_prerelease(octocrab: &Octocrab, repo: &RepoContext) -> Result<Prerelease> {
        let date = chrono::Utc::now();
        let date = date.format("%F").to_string();

        let todays_pre_text = format!("{}.{}", NIGHTLY_BUILD_PREFIX, date);
        let generate_ith = |index: u32| -> Result<Prerelease> {
            let pre = if index == 0 {
                Prerelease::from_str(&todays_pre_text)?
            } else {
                Prerelease::from_str(&format!("{}.{}", todays_pre_text, index))?
            };
            Ok(pre)
        };

        let relevant_nightly_versions = repo
            .all_releases(octocrab)
            .await?
            .into_iter()
            .filter(is_nightly)
            .filter_map(|release| {
                if release.tag_name.contains(&todays_pre_text) {
                    let version = Version::parse(&release.tag_name).ok()?;
                    Some(version.pre)
                } else {
                    None
                }
            })
            .collect::<BTreeSet<_>>();


        // relevant_nightly_versions.last();


        for index in 0.. {
            let pre = generate_ith(index)?;
            if !relevant_nightly_versions.contains(&pre) {
                return Ok(pre);
            }
        }
        unreachable!("After infinite loop.")
    }

    pub fn tag(&self) -> String {
        self.version.to_string()
    }

    pub fn publish(&self) -> Result {
        let name = format!("{}", self.version);
        let edition = self.edition_name();
        ide_ci::actions::workflow::set_output(VERSION_VAR_NAME, &name);
        ide_ci::actions::workflow::set_output(EDITION_VAR_NAME, &edition);

        ide_ci::actions::workflow::set_env(VERSION_VAR_NAME, &name)?;
        ide_ci::actions::workflow::set_env(EDITION_VAR_NAME, &edition)?;
        ide_ci::actions::workflow::set_env(RELEASE_MODE_VAR_NAME, self.release_mode)?;
        Ok(())
    }

    pub fn is_nightly(&self) -> bool {
        self.version.pre.as_str().starts_with(NIGHTLY_BUILD_PREFIX)
    }
}

pub fn version_from_env() -> Result<Version> {
    let version = ide_ci::env::expect_var(VERSION_VAR_NAME)?.parse()?;
    Ok(version)
}

pub fn base_version(changelog_path: impl AsRef<Path>) -> Result<Version> {
    if let Ok(from_env) = version_from_env() {
        return Ok(from_env);
    }

    let changelog_contents = std::fs::read_to_string(changelog_path.as_ref())?;
    let mut headers = crate::changelog::iterate_headers_text(&changelog_contents)
        .map(ide_ci::program::version::find_in_text);

    let version = match headers.next() {
        Some(Ok(version)) => version,
        None => suggest_new_version(),
        Some(Err(_)) => match headers.next() {
            Some(Ok(version)) => suggest_next_version(&version),
            None => suggest_new_version(),
            Some(Err(_)) => bail!("Two leading release headers have no version number in them."),
        },
    };
    Ok(version)
}

impl Display for Versions {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Enso {}", self.version)
    }
}

pub fn current_year() -> u64 {
    chrono::Utc::today().year() as u64
}

pub fn suggest_new_version() -> Version {
    Version::new(current_year(), 1, 1)
}

pub fn suggest_next_version(previous: &Version) -> Version {
    let year = current_year();
    if previous.major == year {
        Version::new(year, previous.minor + 1, 1)
    } else {
        suggest_new_version()
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn iii() -> Result {
        dbg!(base_version(r"H:\nbo\enso\app\gui\changelog.md")?);
        Ok(())
    }
}
