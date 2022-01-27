use crate::paths::Paths;
use crate::prelude::*;
use ide_ci::models::config::RepoContext;

use aws_sdk_s3::Client;

pub async fn update_manifest(repo_context: &RepoContext, paths: &Paths) -> Result {
    println!("Preparing credentials.");
    let config = aws_config::load_from_env().await;
    let client = Client::new(&config);

    let body = dbg!(
        client
            .get_object()
            .bucket("editions.release.enso.org")
            .key("enso/manifest.yaml")
            .send()
            .await
    )?
    .body
    .collect()
    .await?
    .into_bytes();

    println!("{}", std::str::from_utf8(body.as_ref())?);
    // dbg!(client.list_buckets().send().await)?;


    // let credentials = Credentials::from_profile(None)?; //Credentials::new(None, None, None,
    // None, None)?;
    //
    // println!("Instantiating the bucket.");
    // // let bucket = s3::Bucket::new("editions.release.enso.org", Region::UsWest1, credentials)?;
    // let bucket = s3::Bucket::new("editions.release.enso.org", Region::UsWest1, credentials)?;
    //
    // dbg!(bucket.list("".to_string(), None).await)?;
    // let manifest_filename = PathBuf::from("manifest.yaml");
    // let edition_filename = PathBuf::from(format!("{}.yaml", paths.triple.version));

    // let manifest_s3_path = "/{}/"
    // dbg!(bucket.get_object("/enso/manifest.yaml").await);
    Ok(())
}

#[tokio::test]
async fn aaa() -> Result {
    let repo = RepoContext::from_str("enso-org/enso")?;
    let paths = Paths::new(r"H:\NBO\enso")?;
    update_manifest(&repo, &paths).await?;
    Ok(())
}
