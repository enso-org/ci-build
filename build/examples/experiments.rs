use enso_build::prelude::*;
use enso_build::setup_octocrab;
use ide_ci::models::config::RepoContext;
use octocrab::models::ReleaseId;

#[tokio::main]
async fn main() -> Result {
    let octo = setup_octocrab()?;
    let repo = RepoContext::from_str("enso-org/enso-staging")?;
    let handler = repo.repos(&octo);
    let releases = handler.releases();

    let release = releases.get_by_id(ReleaseId(59554885)).await?;
    dbg!(&release);

    Ok(())
}
