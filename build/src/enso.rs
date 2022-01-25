use crate::prelude::*;

use ide_ci::future::AsyncPolicy;
use ide_ci::programs::docker::ContainerId;
use platforms::TARGET_OS;

use crate::paths::Paths;
use crate::postgres;
use crate::postgres::EndpointConfiguration;
use crate::postgres::Postgresql;

#[derive(Copy, Clone, Debug)]
pub enum IrCaches {
    Yes,
    No,
}

impl IrCaches {
    pub fn flag(self) -> &'static str {
        match self {
            IrCaches::Yes => "--ir-caches",
            IrCaches::No => "--no-ir-caches",
        }
    }
}

impl AsRef<OsStr> for IrCaches {
    fn as_ref(&self) -> &OsStr {
        self.flag().as_ref()
    }
}

#[derive(Clone, Debug)]
pub struct BuiltEnso {
    pub paths: Paths,
}

impl BuiltEnso {
    pub fn wrapper_script_path(&self) -> PathBuf {
        self.paths.engine.dir.join("bin").join("enso")
    }

    pub fn run_test(&self, test: impl AsRef<Path>, ir_caches: IrCaches) -> Result<Command> {
        let test_path = self.paths.stdlib_test(test);
        let mut command = self.cmd()?;
        command.arg(ir_caches).arg("--run").arg(test_path);
        Ok(command)
    }

    pub fn compile_lib(&self, target: impl AsRef<Path>) -> Result<Command> {
        let mut command = self.cmd()?;
        command
            .arg(IrCaches::Yes)
            .args(["--no-compile-dependencies", "--no-global-cache", "--compile"])
            .arg(target.as_ref());
        Ok(command)
    }

    pub async fn run_tests(&self, ir_caches: IrCaches, async_policy: AsyncPolicy) -> Result {
        let paths = &self.paths;
        // Prepare Engine Test Environment
        if let Ok(gdoc_key) = std::env::var("GDOC_KEY") {
            let google_api_test_data_dir =
                paths.repo_root.join("test").join("Google_Api_Test").join("data");
            ide_ci::io::create_dir_if_missing(&google_api_test_data_dir)?;
            std::fs::write(google_api_test_data_dir.join("secret.json"), &gdoc_key)?;
        }

        let _httpbin = crate::httpbin::get_and_spawn_httpbin_on_free_port().await?;
        let _postgres = match TARGET_OS {
            OS::Linux => {
                let runner_context_string = ide_ci::actions::env::runner_name()
                    .unwrap_or_else(|_| Uuid::new_v4().to_string());
                // GH-hosted runners are named like "GitHub Actions 10". Spaces are not allowed in
                // the container name.
                let container_name =
                    iformat!("postgres-for-{runner_context_string}").replace(' ', "_");
                let config = postgres::Configuration {
                    postgres_container: ContainerId(container_name),
                    database_name:      "enso_test_db".to_string(),
                    user:               "enso_test_user".to_string(),
                    password:           "enso_test_password".to_string(),
                    endpoint:           EndpointConfiguration::deduce()?,
                    version:            "latest".to_string(),
                };
                let postgres = Postgresql::start(config).await?;
                Some(postgres)
            }
            _ => None,
        };

        let futures = crate::paths::LIBRARIES_TO_TEST.map(ToString::to_string).map(|test| {
            let command = self.run_test(test, ir_caches);
            async move { command?.run_ok().await }
        });

        let _result = ide_ci::future::try_join_all(futures, async_policy).await?;

        // We need to join all the test tasks here, as they require postgres and httpbin alive.
        // Could share them with Arc but then scenario of multiple test runs being run in parallel
        // should be handled, e.g. avoiding port collisions.
        Ok(())
    }
}

#[async_trait]
impl Program for BuiltEnso {
    fn executable_name() -> &'static str {
        ide_ci::platform::DefaultShell::executable_name()
    }

    fn cmd(&self) -> Result<Command> {
        ide_ci::platform::default_shell().run_script(self.wrapper_script_path())
    }

    async fn version_string(&self) -> Result<String> {
        let output = self.cmd()?.args(["version", "--json", "--only-launcher"]).output().await?;
        output.status.exit_ok().map_err(|e| {
            anyhow!(
                "Failed to get version: {}. \nStdout: {}\nStderr: {}",
                e,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
        })?;
        String::from_utf8(output.stdout).anyhow_err()
    }

    async fn version(&self) -> Result<Version> {
        #[derive(Clone, Debug, Deserialize)]
        struct VersionInfo {
            version: Version,
        }

        let stdout = self.version_string().await?;
        let version = serde_json::from_str::<VersionInfo>(&stdout)?;
        Ok(version.version)
    }
}