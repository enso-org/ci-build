#![feature(default_free_fn)]

use enso_build::prelude::*;
use enso_build::setup_octocrab;

use ide_ci::actions::artifacts;
use ide_ci::actions::artifacts::FileToUpload;

#[tokio::main]
async fn main() -> Result {
    let dir = std::env::current_exe()?.parent().unwrap().to_owned();

    println!("Will upload {}", dir.display());
    let provider = artifacts::discover_recursive(dir);
    artifacts::upload_artifact(provider, "MyPrecious", default()).await?;


    let file = std::env::current_exe()?;
    println!("Will upload {}", file.display());
    let file = FileToUpload::new(file)?;
    let artifact_name = file.remote_path.as_str().to_owned();
    let provider = futures::stream::iter([file]);
    artifacts::upload_artifact(provider, artifact_name, default()).await?;
    // artifacts::upload_single_file(file, )

    let octocrat = setup_octocrab()?;
    let run_id = ide_ci::actions::env::run_id()?;
    let run = octocrat.workflows("enso-org", "ci-build").get(run_id).await;
    dbg!(run);
    let artifacts = octocrat.actions().list_workflow_run_artifacts("enso-org", "ci-build", run_id).send().await;
    dbg!(artifacts);

    let context = artifacts::Context::new()?;
    let list = ide_ci::actions::artifacts::raw::list_artifacts(&context.json_client()?, context.artifact_url()?).await?;
    dbg!(list);

    Ok(())
}
