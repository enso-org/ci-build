use crate::prelude::*;

use ide_ci::env::Variable;
use ide_ci::programs::Go;
use tokio::process::Child;

pub mod env {
    /// Environment variable that stores URL under which spawned httpbin server is available.
    pub struct Url;
    impl ide_ci::env::Variable for Url {
        const NAME: &'static str = "ENSO_HTTP_TEST_HTTPBIN_URL";
        type Value = url::Url;
    }
}

pub struct Spawned {
    pub process: Child,
    pub url:     Url,
}

pub async fn get_and_spawn_httpbin(port: u16) -> Result<Spawned> {
    Go.call_args(["get", "-v", "github.com/ahmetb/go-httpbin/cmd/httpbin"]).await?;
    let gopath = Go.cmd()?.args(["env", "GOPATH"]).run_stdout().await?;
    let gopath = gopath.trim();
    let gopath = PathBuf::from(gopath); // be careful of trailing newline!
    let program = gopath.join("bin").join("httpbin");
    debug!("Will spawn {}", program.display());
    let process = tokio::process::Command::new(program) // TODO? wrap in Program?
        .args(["-host", &iformat!(":{port}")])
        .kill_on_drop(true)
        .spawn()
        .anyhow_err()?;

    let url_string = iformat!("http://localhost:{port}");
    let url = Url::parse(&url_string)?;
    env::Url.set(&url);
    Ok(Spawned { url, process })
}

impl Drop for Spawned {
    fn drop(&mut self) {
        env::Url.remove();
    }
}

pub async fn get_and_spawn_httpbin_on_free_port() -> Result<Spawned> {
    get_and_spawn_httpbin(ide_ci::get_free_port()?).await
}
