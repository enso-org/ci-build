#![feature(default_free_fn)]

use enso_build::prelude::*;

use ide_ci::log::setup_logging;

#[tokio::main]
async fn main() -> Result {
    setup_logging()?;

    println!("Hello");
    trace!("Hello");
    debug!("Hello");
    info!("Hello");
    warn!("Hello");
    error!("Hello");
    Ok(())
}
