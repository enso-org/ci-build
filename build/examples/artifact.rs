use anyhow::Context;
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

    let dir = std::env::current_dir()?;
    let (tx, rx) = flume::unbounded();
    tokio::task::spawn_blocking(move || {
        for entry in walkdir::WalkDir::new(dir) {
            match entry {
                Ok(entry) => {
                    tx.send(entry.into_path()).unwrap();
                }
                e => {
                    e.context(anyhow!(
                        "Scanning directory {} encountered an error.",
                        dir.display()
                    ));
                    break;
                }
            }
        }
    });


    artifacts::upload_artifact(rx.stream(), "MyCargoArtifact").await?;
    // artifacts::upload_path(path_to_upload).await?;
    Ok(())
    //let client = reqwest::Client::builder().default_headers().
}
