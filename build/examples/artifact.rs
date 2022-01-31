use enso_build::prelude::*;

use ide_ci::actions::artifacts;
use ide_ci::actions::artifacts::FileToUpload;

#[tokio::main]
async fn main() -> Result {
    let path_to_upload = "Cargo.toml";

    let file_to_upload = FileToUpload {
        local_path:  PathBuf::from(path_to_upload),
        remote_path: PathBuf::from(path_to_upload),
    };

    artifacts::upload_artifact(futures::stream::once(ready(file_to_upload)), "MyCargoArtifact")
        .await?;
    // artifacts::upload_path(path_to_upload).await?;
    Ok(())
    //let client = reqwest::Client::builder().default_headers().
}
