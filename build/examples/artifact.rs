use enso_build::prelude::*;

use ide_ci::actions::artifacts;

#[tokio::main]
async fn main() -> Result {
    let dir = std::env::current_exe()?.parent().unwrap().to_owned();

    println!("Will upload {}", dir.display());
    let provider = artifacts::discover_recursive(dir);
    artifacts::upload_artifact(provider, "MyPrecious").await?;
    Ok(())
}
