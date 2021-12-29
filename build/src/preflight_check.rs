use crate::prelude::*;
use octocrab::models::repos::Release;
use regex::Regex;
use semver::Prerelease;

const OWNER: &str = "enso-org";
const REPO: &str = "enso"; // FIXME
const MAX_PER_PAGE: u8 = 100;
const NIGHTLY_RELEASE_TITLE_INFIX: &str = "Nightly";

pub struct PreflightCheckOutput {
    pub proceed:         bool,
    pub enso_version:    Version,
    pub edition_version: Version,
}

pub fn is_nightly(release: &Release) -> bool {
    !release.draft
        && release.name.as_ref().map_or(false, |name| name.contains(NIGHTLY_RELEASE_TITLE_INFIX))
}

pub async fn nightly_releases(octocrab: &Octocrab) -> Result<Vec<Release>> {
    let repo = octocrab.repos(OWNER, REPO);
    let mut page = repo.releases().list().per_page(MAX_PER_PAGE).send().await?;
    // TODO: rate limit?
    let releases = octocrab.all_pages(page).await?.into_iter().filter(is_nightly);
    Ok(releases.collect())
}

/// Checks if there are any new changes to see if the nightly build should proceed.
pub fn check_proceed(current_head_sha: &str, nightlies: &[Release]) -> bool {
    if let Some(latest_nightly) = nightlies.first() {
        if latest_nightly.target_commitish == current_head_sha {
            println!("Current commit ({}) is the same as for the most recent nightly build. A new build is not needed.", current_head_sha);
            false
        } else {
            println!("Current commit ({}) is different from the most recent nightly build ({}). Proceeding with a new nightly build.", current_head_sha, latest_nightly.target_commitish);
            true
        }
    } else {
        println!("No prior nightly releases found. Proceeding with the first release.");
        true
    }
}

/// Prepares a version string and edition name for the nightly build.
///
/// A `-SNAPSHOT` suffix is added if it is not already present, next the current
/// date is appended. If this is not the first nightly build on that date, an
/// increasing numeric suffix is added.
pub fn prepare_version(repo_root: impl AsRef<Path>, nightlies: &[Release]) -> Result<Version> {
    let is_taken = |suffix: &str| nightlies.iter().any(|entry| entry.tag_name.ends_with(suffix));
    let build_sbt_path = repo_root.as_ref().join("build.sbt");
    let build_sbt_content = std::fs::read_to_string(&build_sbt_path)?;

    let found_version = enso_build::get_enso_version(&build_sbt_content)?;


    let date = chrono::Utc::now().format("%F").to_string();
    let generate_nightly_identifier = |index: u32| {
        if index == 0 {
            date.clone()
        } else {
            format!("{}.{}", date, index)
        }
    };
    //
    // let relevant_nightly_versions = nightlies.into_iter().filter_map(|release| {
    //
    //     let version_str = release.tag_name.strip_prefix("enso-").unwrap_or(&release.tag_name);
    //     let version = Version::parse(version_str).unwrap();
    //     todo!()
    // });
    //
    // // let relevant_releases = nightlies.into_iter().filter(|r| r.name.contains(&date));
    //
    //
    // for index in 0.. {
    //     let nightly = generate_nightly_identifier(index);
    //     let prerelease_text = format!("SNAPSHOT.{}", nightly);
    // }



    // let mut version = found_version.clone();
    // for suffix in 0.. {
    //     let mut prerelease_text = if suffix == 0 {
    //         prerelease_text.clone()
    //     } else {
    //         format!("{}.{}", prerelease_text, suffix)
    //     };
    //     version.pre = Prerelease::new(&prerelease_text)?;
    //     if !is_taken(&version.to_string()) {
    //         break;
    //     }
    // }



    //
    // if version.pre.as_str() == "SNAPSHOT" {
    //     version.pre = Prerelease::new(version.pre.to_string() + "SNAPSHOT")?;
    // };

    Ok(found_version)

    // const version = match[1]
    // let baseName = version
    // if (!baseName.endsWith('SNAPSHOT')) {
    //     baseName += '-SNAPSHOT'
    // }
    //
    // const now = isoDate()
    // function makeSuffix(ix) {
    //     if (ix == 0) {
    //         return now
    //     } else {
    //         return now + '.' + ix
    //     }
    // }
    //
    // let ix = 0
    // while (isTaken(makeSuffix(ix))) {
    //     ix++
    // }
    //
    // const suffix = makeSuffix(ix)
    // const versionName = baseName + '.' + suffix
    // const edition = 'nightly-' + suffix
    // console.log("The build will be using version '" + versionName + "'")
    // console.log("The build will be using edition '" + edition + "'")
    // return {
    //     version: versionName,
    //     edition: edition,
    // }
}

// async function main() {
//     const nightlies = await github.fetchNightlies()
//     const shouldProceed = checkProceed(nightlies)
//     setProceed(shouldProceed)
//     if (shouldProceed) {
//         const versions = prepareVersions(nightlies)
//         setVersionString(versions.version)
//         setEditionName(versions.edition)
//     }
// }
//
// main().catch(err => {
//     console.error(err)
//     process.exit(1)
// })


#[cfg(test)]
mod tests {
    use super::*;
    use ide_ci::programs::git::Git;

    #[tokio::test]
    async fn foo() -> Result {
        let octocrab = Octocrab::default();
        let repo_path = PathBuf::from(r"H:\NBO\enso");
        // let git = Git::new(&repo_path);
        dbg!(prepare_version(&repo_path, &[]))?;

        // dbg!(git.head_hash().await);
        // ide_ci::programs::git::Git::dbg!(nightly_releases(&octocrab).await?);
        Ok(())
    }
}
