use crate::prelude::*;

use ide_ci::programs::Go;
use std::process::Stdio;
use tokio::process::Child;

pub struct Spawned {
    pub process: Child,
    pub url:     Url,
}

pub async fn get_and_spawn_httpbin(port: u16) -> Result<Spawned> {
    Go.call_args(["get", "-v", "github.com/ahmetb/go-httpbin/cmd/httpbin"]).await?;
    let gopath = String::from_utf8(
        Go.cmd()?.args(["env", "GOPATH"]).stdout(Stdio::piped()).output().await?.stdout,
    )?;
    let gopath = gopath.trim();
    let gopath = PathBuf::from(gopath); // be careful of trailing newline!
    let program = gopath.join("bin").join("httpbin");
    println!("Will spawn {}", program.display());
    let process = Command::new(program)
        .args(["-host", &iformat!(":{port}")])
        .kill_on_drop(true)
        .spawn()
        .anyhow_err()?;

    let url_string = iformat!("http://localhost:{port}");
    let url = Url::parse(&url_string)?;
    std::env::set_var("ENSO_HTTP_TEST_HTTPBIN_URL", &url_string);
    Ok(Spawned { url, process })
}

impl Drop for Spawned {
    fn drop(&mut self) {
        std::env::remove_var("ENSO_HTTP_TEST_HTTPBIN_URL");
    }
}

pub async fn get_and_spawn_httpbin_on_free_port() -> Result<Spawned> {
    get_and_spawn_httpbin(ide_ci::get_free_port()?).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn spawn_httpbin() -> Result {
        let mut httpbin = get_and_spawn_httpbin_on_free_port().await?;
        Command::new("cmd")
            .args(["/c", "H:\\NBO\\enso2\\built-distribution\\enso-engine-0.2.32-SNAPSHOT-windows-amd64\\enso-0.2.32-SNAPSHOT\\bin\\enso", "--no-ir-caches", "--run", "H:\\NBO\\enso2\\test\\Tests"]).run_ok().await?;
        httpbin.process.kill().await?;
        Ok(())
    }
}
