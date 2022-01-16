use crate::prelude::*;
use std::collections::HashMap;

use ide_ci::programs::docker::ImageId;
use ide_ci::programs::docker::RunOptions;
use ide_ci::programs::Docker;
use std::process::Stdio;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncRead;
use tokio::io::BufReader;
use tokio::process::Child;

/// Port used by Postgres in its container.
const POSTGRES_CONTAINER_DEFAULT_PORT: u16 = 5432;

pub struct Configuration {
    pub container_name: String,
    pub database_name:  String,
    pub user:           String,
    pub password:       String,
    pub port:           u16,
    pub version:        String,
}

impl Configuration {
    pub fn image_id(&self) -> ImageId {
        ImageId(format!("postgres:{}", &self.version))
    }

    pub fn enso_test_env(&self) -> HashMap<&str, String> {
        [
            ("ENSO_DATABASE_TEST_DB_NAME", self.database_name.clone()),
            ("ENSO_DATABASE_TEST_HOST", iformat!("localhost:{self.port}")),
            ("ENSO_DATABASE_TEST_DB_USER", self.user.clone()),
            ("ENSO_DATABASE_TEST_DB_PASSWORD", self.password.clone()),
        ]
        .into_iter()
        .collect()
    }

    pub fn set_enso_test_env(&self) {
        for (name, val) in self.enso_test_env() {
            std::env::set_var(name, val);
        }
    }

    pub fn clear_enso_test_env(&self) {
        for (name, _) in self.enso_test_env() {
            std::env::remove_var(name);
        }
    }

    pub async fn cleanup(&self) -> Result {
        Docker.remove_container(&self.container_name, true).await
    }
}

/// Retrieve input from asynchronous reader line by line and feed them into the given function.
pub async fn process_lines<R: AsyncRead + Unpin>(reader: R, f: impl Fn(String)) -> Result<R> {
    println!("Started line processor.");
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    while reader.read_line(&mut line).await? != 0 {
        f(std::mem::take(&mut line));
    }
    Ok(reader.into_inner())
}

pub async fn process_lines_until<R: AsyncRead + Unpin>(
    reader: R,
    f: &impl Fn(&str) -> bool,
) -> Result<R> {
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    loop {
        let bytes_read = reader.read_line(&mut line).await?;
        ensure!(bytes_read != 0, "Postgresql container closed without being ready!");
        if f(&line) {
            break;
        }
        line.clear();
    }
    Ok(reader.into_inner())
}

pub struct PostgresContainer {
    _docker_run: Child,
    config:      Configuration,
}

impl Drop for PostgresContainer {
    fn drop(&mut self) {
        self.config.clear_enso_test_env();

        println!("Will remove the postgres container");
        let cleanup_future = self.config.cleanup();
        if let Err(e) = futures::executor::block_on(cleanup_future) {
            println!(
                "Failed to kill the Postgres container named {}: {}",
                self.config.container_name, e
            );
        } else {
            println!("Postgres container killed.");
        }
    }
}

pub struct Postgresql;

impl Postgresql {
    pub async fn start(config: Configuration) -> Result<PostgresContainer> {
        // Attempt cleanup in case previous script run crashed in the middle of this.
        // Otherwise, postgres container names could collide.
        let _ = config.cleanup().await;

        let mut opts = RunOptions::new(config.image_id());
        opts.env("POSTGRES_DB", &config.database_name);
        opts.env("POSTGRES_USER", &config.user);
        opts.env("POSTGRES_PASSWORD", &config.password);
        opts.publish_port(config.port, POSTGRES_CONTAINER_DEFAULT_PORT);
        opts.sig_proxy = Some(true);
        opts.name = Some(config.container_name.clone());

        let mut cmd = Docker.run_cmd(&opts)?;
        cmd.stderr(Stdio::piped());
        cmd.kill_on_drop(true);
        let mut child = cmd.spawn_nicer().anyhow_err()?;
        let stderr = child
            .stderr
            .ok_or_else(|| anyhow!("Failed to access standard output of the spawned process!"))?;

        // Wait until container is ready.
        let check_line = |line: &str| {
            println!("ERR: {}", line);
            line.contains("database system is ready to accept connections")
        };
        let stderr = process_lines_until(stderr, &check_line).await?;

        // Put back stream we've been reading and pack the whole thing back for the caller.
        child.stderr = Some(stderr);

        config.set_enso_test_env();
        Ok(PostgresContainer { _docker_run: child, config })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ide_ci::get_free_port;

    #[tokio::test]
    #[ignore]
    async fn start_postgres() -> Result {
        let config = Configuration {
            container_name: "something".into(),
            port:           get_free_port()?,
            version:        "latest".into(),
            user:           "test".into(),
            password:       "test".into(),
            database_name:  "test".into(),
        };
        let child = Postgresql::start(config).await?;
        // drop(child);
        std::mem::forget(child);
        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn test_postgres() -> Result {
        let config = Configuration {
            container_name: "something".into(),
            port:           get_free_port()?,
            version:        "latest".into(),
            user:           "test".into(),
            password:       "test".into(),
            database_name:  "test".into(),
        };
        let child = Postgresql::start(config).await?;
        std::mem::forget(child);
        // let mut httpbin = get_and_spawn_httpbin_on_free_port().await?;
        Command::new("cmd")
            .args(["/c", "H:\\NBO\\enso2\\built-distribution\\enso-engine-0.2.32-SNAPSHOT-windows-amd64\\enso-0.2.32-SNAPSHOT\\bin\\enso", "--no-ir-caches", "--run", "H:\\NBO\\enso2\\test\\Database_Tests"]).run_ok().await?;
        // httpbin.process.kill().await?;
        Ok(())
    }
}
