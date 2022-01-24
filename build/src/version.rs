// use crate::preflight_check::NIGHTLY_RELEASE_TITLE_INFIX;
use crate::prelude::*;
use ide_ci::models::config::RepoContext;
use octocrab::models::repos::Release;
use semver::Prerelease;
use std::collections::BTreeSet;
use std::str::FromStr;

/// Variable that stores Enso Engine version.
const VERSION_VAR_NAME: &str = "ENSO_VERSION";
const EDITION_VAR_NAME: &str = "ENSO_EDITION";
const RELEASE_MODE_VAR_NAME: &str = "ENSO_RELEASE_MODE";

const DEV_BUILD_PREFIX: &str = "dev";
const NIGHTLY_BUILD_PREFIX: &str = "nightly";
// pub enum Kind {
//     Local,
//     Nightly,
//     // Rc,
//     // Stable,
// }

pub fn default_engine_version() -> Version {
    let mut ret = Version::new(0, 0, 0);
    ret.pre = Prerelease::new(DEV_BUILD_PREFIX).unwrap();
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
        let release_mode = version.pre.as_str().contains(DEV_BUILD_PREFIX);
        Versions { version, release_mode }
    }

    pub async fn new_nightly(octocrab: &Octocrab, repo: &RepoContext) -> Result<Prerelease> {
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

    pub fn publish(&self) -> Result {
        let name = format!("{}", self.version);
        ide_ci::actions::workflow::set_output(VERSION_VAR_NAME, &name);
        ide_ci::actions::workflow::set_output(EDITION_VAR_NAME, &name);

        ide_ci::actions::workflow::set_env(VERSION_VAR_NAME, &name)?;
        ide_ci::actions::workflow::set_env(EDITION_VAR_NAME, &name)?;
        ide_ci::actions::workflow::set_env(RELEASE_MODE_VAR_NAME, self.release_mode)?;
        Ok(())
    }

    pub fn from_env() -> Result<Self> {
        let version = ide_ci::env::expect_var(VERSION_VAR_NAME)?.parse()?;
        Ok(Versions::new(version))
    }

    pub fn is_nightly(&self) -> bool {
        self.version.pre.as_str().starts_with(NIGHTLY_BUILD_PREFIX)
    }
}

// #[tokio::test]
// #[ignore]
// async fn aaaa() -> Result {
//     let octocrab = crate::setup_octocrab()?;
//     let releases = octocrab.repos("enso-org", "ci-build").releases();
//     dbg!(Versions::new_nightly(&releases).await?);
//     Ok(())
// }
