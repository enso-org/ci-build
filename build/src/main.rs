#![feature(default_free_fn)]

use enso_build::prelude::*;

use enso_build::args::Args;
use enso_build::engine::RunContext;
use ide_ci::actions::artifacts;


#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let dir = std::env::current_exe()?.parent().unwrap().to_owned();

    debug!("Will upload {}", dir.display());
    let provider = artifacts::discover_recursive(dir);
    artifacts::upload(provider, "MyPrecious", default()).await?;
    Ok(())
}
