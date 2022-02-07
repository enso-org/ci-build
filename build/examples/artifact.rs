use enso_build::prelude::*;

use ide_ci::actions::artifacts;
use ide_ci::actions::artifacts::FileToUpload;

#[tokio::main]
async fn main() -> Result {
    let dir = std::env::current_exe()?.parent().unwrap().to_owned();

    println!("Will upload {}", dir.display());
    let provider = artifacts::discover_recursive(dir);
    artifacts::upload_artifact(provider, "MyPrecious").await?;


    let file = std::env::current_exe()?;
    println!("Will upload {}", file.display());
    let file = FileToUpload::new(file)?;
    let artifact_name = file.remote_path.as_str().to_owned();
    let provider = futures::stream::iter([file]);
    artifacts::upload_artifact(provider, artifact_name).await?;
    // artifacts::upload_single_file(file, )

    Ok(())
}
