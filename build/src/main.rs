use enso_build::prelude::*;

use enso_build::args::Args;
use enso_build::engine::RunContext;


#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // We want arg parsing to be the very first thing, so when user types wrong arguments, the error
    // diagnostics will be first and only thing that is output.
    let args: Args = argh::from_env();

    debug!("Initial environment:");
    for (key, value) in std::env::vars() {
        debug!("\t{key}={value}");
    }
    debug!("\n===End of the environment dump===\n");

    let ctx = RunContext::new(&args).await?;
    ctx.execute().await?;
    Ok(())
}
