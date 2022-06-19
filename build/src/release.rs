use crate::prelude::*;

use crate::context::BuildContext;
use crate::paths::EDITION_FILE_ARTIFACT_NAME;
use octocrab::models::repos::Release;
use tempfile::tempdir;

pub async fn create_release(context: &BuildContext) -> Result<Release> {
    let versions = &context.triple.versions;
    let commit = ide_ci::actions::env::GITHUB_SHA.get()?;

    let paths = context.repo_root();
    let changelog_contents = ide_ci::fs::read_to_string(&paths.changelog_md)?;
    let latest_changelog_body =
        crate::changelog::Changelog(&changelog_contents).top_release_notes()?;

    debug!("Preparing release {} for commit {}", versions.version, commit);
    let release = context
        .remote_repo
        .repos(&context.octocrab)
        .releases()
        .create(&versions.tag())
        .target_commitish(&commit)
        .name(&versions.pretty_name())
        .body(&latest_changelog_body.contents)
        .prerelease(true)
        .draft(true)
        .send()
        .await?;

    crate::env::ReleaseId.emit(&release.id)?;
    Ok(release)
}

pub async fn publish_release(context: &BuildContext) -> Result {
    let BuildContext { remote_repo, octocrab, triple, .. } = context;

    let release_id = crate::env::ReleaseId.fetch()?;

    debug!("Looking for release with id {release_id} on github.");
    let release = remote_repo.repos(octocrab).releases().get_by_id(release_id).await?;
    ensure!(release.draft, "Release has been already published!");

    debug!("Found the target release, will publish it.");
    remote_repo.repos(octocrab).releases().update(release.id.0).draft(false).send().await?;
    debug!("Done. Release URL: {}", release.url);

    let temp = tempdir()?;
    let edition_file_path = crate::paths::generated::RepoRootDistributionEditions::new_root(
        temp.path(),
        triple.versions.edition_name(),
    )
    .edition_yaml;


    ide_ci::actions::artifacts::download_single_file_artifact(
        EDITION_FILE_ARTIFACT_NAME,
        &edition_file_path,
    )
    .await?;

    debug!("Updating edition in the AWS S3.");
    crate::aws::update_manifest(remote_repo, &edition_file_path).await?;

    Ok(())
}
