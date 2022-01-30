use enso_build::prelude::*;

use ide_ci::actions::artifacts;

#[tokio::main]
async fn main() -> Result {
    let path_to_upload = "Cargo.toml";

    artifacts::upload_path(path_to_upload).await?;
    Ok(())
    //let client = reqwest::Client::builder().default_headers().
}
