#![feature(default_free_fn)]

use enso_build::prelude::*;
use enso_build::setup_octocrab;
use reqwest::header::HeaderMap;
use tempfile::TempDir;

use ide_ci::actions::artifacts;
use ide_ci::actions::artifacts::download::FileToDownload;
use ide_ci::actions::artifacts::models::ItemType;
use ide_ci::actions::artifacts::raw;
use ide_ci::actions::artifacts::run_session::SessionClient;

#[tokio::main]
async fn main() -> Result {
    let dir = std::env::current_exe()?.parent().unwrap().to_owned();

    println!("Will upload {}", dir.display());
    let provider = artifacts::discover_recursive(dir);
    artifacts::upload_artifact(provider, "MyPrecious", default()).await?;


    let file = std::env::current_exe()?;
    println!("Will upload {}", file.display());
    let artifact_name = file.file_name().unwrap().to_str().unwrap();
    let provider = artifacts::single_file_provider(file.clone())?;
    artifacts::upload_artifact(provider, artifact_name, default()).await?;
    // artifacts::upload_single_file(file, )

    let octocrab = setup_octocrab()?;
    let context = artifacts::context::Context::new_from_env()?;
    let session = SessionClient::new(&context)?;

    let run_id = ide_ci::actions::env::run_id()?;
    let run = octocrab.workflows("enso-org", "ci-build").get(run_id).await;
    dbg!(run)?;
    let artifacts =
        octocrab.actions().list_workflow_run_artifacts("enso-org", "ci-build", run_id).send().await;
    dbg!(artifacts)?;


    let list = session.list_artifacts().await?;
    dbg!(&list);

    let relevant_entry = list
        .iter()
        .find(|artifact| artifact.name == artifact_name)
        .ok_or_else(|| anyhow!("Failed to find artifact by name {artifact_name}."))?;

    dbg!(&relevant_entry);

    let items = ide_ci::actions::artifacts::raw::endpoints::get_container_items(
        &context.json_client()?,
        relevant_entry.file_container_resource_url.clone(),
        &relevant_entry.name,
    )
    .await?;
    dbg!(&items);

    let download_client = context.download_client()?;
    let temp = TempDir::new()?;
    for item in items.value {
        if item.item_type == ItemType::File {
            dbg!(FileToDownload::new(temp.path(), &item, &relevant_entry.name));
            let destination = temp.path().join(item.relative_path());
            raw::endpoints::download_item(
                &download_client,
                item.content_location.clone(),
                &destination,
            )
            .await?;

            dbg!(destination.metadata());
        }
    }



    Ok(())
}
