use crate::prelude::*;

use crate::paths::generated::RepoRoot;
use crate::paths::generated::RepoRootDistWasm;
use crate::project::wasm::js_patcher::patch_js_glue_in_place;
use crate::project::IsArtifact;
use crate::project::IsTarget;
use crate::project::IsWatchable;

use anyhow::Context;
use derivative::Derivative;
use ide_ci::env::Variable;
use ide_ci::programs::cargo;
use ide_ci::programs::wasm_pack;
use ide_ci::programs::Cargo;
use ide_ci::programs::WasmPack;
use semver::VersionReq;
use std::time::Duration;
use tempfile::tempdir;
use tokio::process::Child;

pub mod env;
pub mod js_patcher;
pub mod test;

pub const INTEGRATION_TESTS_WASM_TIMEOUT: Duration = Duration::from_secs(300);
pub const WASM_ARTIFACT_NAME: &str = "gui_wasm";
pub const OUTPUT_NAME: &str = "ide";
pub const TARGET_CRATE: &str = "app/gui";
pub const WASM_PACK_VERSION_REQ: &str = ">=0.10.1";

#[derive(Clone, Copy, Debug, strum::Display, strum::EnumString, PartialEq)]
#[strum(serialize_all = "kebab-case")]
pub enum ProfilingLevel {
    Objective,
    Task,
    Details,
    Debug,
}

#[derive(Clone, Derivative)]
#[derivative(Debug)]
pub struct BuildInput {
    #[derivative(Debug(format_with = "std::fmt::Display::fmt"))]
    pub repo_root:           RepoRoot,
    /// Path to the crate to be compiled to WAM. Relative to the repository root.
    pub crate_path:          PathBuf,
    pub extra_cargo_options: Vec<String>,
    pub profile:             wasm_pack::Profile,
    pub profiling_level:     Option<ProfilingLevel>,
    pub wasm_size_limit:     Option<byte_unit::Byte>,
}

impl BuildInput {
    pub async fn perhaps_check_size(&self, wasm_path: impl AsRef<Path>) -> Result {
        if let Some(wasm_size_limit) = self.wasm_size_limit {
            if self.profile != wasm_pack::Profile::Release {
                warn!(
                    "Skipping size check because profile={} rather than {}.",
                    self.profile,
                    wasm_pack::Profile::Release
                );
            } else if self.profiling_level.map_or(false, |level| level != ProfilingLevel::Objective)
            {
                warn!(
                    "Skipping size check because profiling level={:?} rather than {}.",
                    self.profiling_level,
                    ProfilingLevel::Objective
                );
            } else {
                let actual_size = compressed_size(&wasm_path).await?;
                info!(
                    "Checking that {} size: {} <= {} (limit).",
                    wasm_path.as_ref().display(),
                    actual_size.get_appropriate_unit(true),
                    wasm_size_limit.get_appropriate_unit(true)
                );
                ensure!(actual_size < wasm_size_limit)
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wasm;

#[async_trait]
impl IsTarget for Wasm {
    type BuildInput = BuildInput;
    type Artifact = Artifact;

    fn artifact_name(&self) -> String {
        WASM_ARTIFACT_NAME.into()
    }

    fn adapt_artifact(self, path: impl AsRef<Path>) -> BoxFuture<'static, Result<Self::Artifact>> {
        ready(Ok(Artifact::new(path.as_ref()))).boxed()
    }

    fn build_locally(
        &self,
        input: Self::BuildInput,
        output_path: impl AsRef<Path> + Send + Sync + 'static,
    ) -> BoxFuture<'static, Result<Self::Artifact>> {
        let span = info_span!("Building WASM.",
            repo = %input.repo_root.display(),
            crate = %input.crate_path.display(),
            cargo_opts = ?input.extra_cargo_options
        );
        async move {
            // Old wasm-pack does not pass trailing `build` command arguments to the Cargo.
            // We want to be able to pass --profile this way.
            WasmPack.require_present_that(&VersionReq::parse(">=0.10.1")?).await?;

            let BuildInput {
                repo_root,
                crate_path,
                extra_cargo_options,
                profile,
                profiling_level,
                wasm_size_limit: _wasm_size_limit,
            } = &input;

            info!("Building wasm.");
            let temp_dir = tempdir()?;
            let temp_dist = RepoRootDistWasm::new(temp_dir.path());
            let mut command = ide_ci::programs::WasmPack.cmd()?;
            command
                .current_dir(&repo_root)
                .kill_on_drop(true)
                .env_remove(ide_ci::programs::rustup::env::Toolchain::NAME)
                .set_env(env::ENSO_ENABLE_PROC_MACRO_SPAN, &true)?
                .build()
                .arg(&profile)
                .target(wasm_pack::Target::Web)
                .output_directory(&temp_dist)
                .output_name(&OUTPUT_NAME)
                .arg(&crate_path)
                .arg("--")
                .apply(&cargo::Color::Always)
                .args(extra_cargo_options);

            if let Some(profiling_level) = profiling_level {
                command.set_env(env::ENSO_MAX_PROFILING_LEVEL, &profiling_level)?;
            }
            command.run_ok().await?;

            patch_js_glue_in_place(&temp_dist.wasm_glue)?;
            ide_ci::fs::rename(&temp_dist.wasm_main_raw, &temp_dist.wasm_main)?;

            ide_ci::fs::create_dir_if_missing(&output_path)?;
            let ret = RepoRootDistWasm::new(output_path.as_ref());
            ide_ci::fs::copy(&temp_dist, &ret)?;
            // copy_if_different(&temp_dist.wasm_glue, &ret.wasm_glue)?;
            // copy_if_different(&temp_dist.wasm_main_raw, &ret.wasm_main)?;
            input.perhaps_check_size(&ret.wasm_main).await?;
            Ok(Artifact(ret))
        }
        .instrument(span)
        .boxed()
    }
}

pub fn check_if_identical(source: impl AsRef<Path>, target: impl AsRef<Path>) -> bool {
    (|| -> Result<bool> {
        if ide_ci::fs::metadata(&source)?.len() == ide_ci::fs::metadata(&target)?.len() {
            Ok(true)
        } else if ide_ci::fs::read(&source)? == ide_ci::fs::read(&target)? {
            // TODO: Not good for large files, should process them chunk by chunk.
            Ok(true)
        } else {
            Ok(false)
        }
    })()
    .unwrap_or(false)
}

pub fn copy_if_different(source: impl AsRef<Path>, target: impl AsRef<Path>) -> Result {
    if !check_if_identical(&source, &target) {
        ide_ci::fs::copy(&source, &target)?;
    }
    Ok(())
}

impl IsWatchable for Wasm {
    type Watcher = crate::project::Watcher<Self, Child>;

    fn setup_watcher(
        &self,
        input: Self::BuildInput,
        output_path: impl AsRef<Path> + Send + Sync + 'static,
    ) -> BoxFuture<'static, Result<Self::Watcher>> {
        // TODO
        // This is not nice, as this module should not be aware of the CLI parsing/generation.
        // Rather than using `cargo watch` this should be implemented directly in Rust.
        async move {
            let BuildInput {
                repo_root,
                crate_path,
                extra_cargo_options,
                profile,
                profiling_level,
                wasm_size_limit,
            } = input;

            let current_exe = std::env::current_exe()?;
            // Cargo watch apparently cannot handle extended-length UNC path prefix.
            // We remove it and hope for the best.
            let current_exe = current_exe.without_verbatim_prefix();


            let mut watch_cmd = Cargo.cmd()?;

            watch_cmd
                .kill_on_drop(true)
                .current_dir(&repo_root)
                .arg("watch")
                .args(["--ignore", "README.md"])
                .arg("--")
                // FIXME: does not play nice for use as a library
                .arg(current_exe)
                .args(["--repo-path", repo_root.as_str()])
                .arg("wasm")
                .arg("build")
                .args(["--crate-path", crate_path.as_str()])
                .args(["--wasm-output-path", output_path.as_str()])
                .args(["--wasm-profile", profile.as_ref()]);
            if let Some(profiling_level) = profiling_level {
                watch_cmd.args(["--profiling-level", profiling_level.to_string().as_str()]);
            }
            if let Some(wasm_size_limit) = wasm_size_limit {
                watch_cmd.args(["--wasm-size-limit", wasm_size_limit.to_string().as_str()]);
            }
            watch_cmd.arg("--").args(extra_cargo_options);

            let watch_process = watch_cmd.spawn_intercepting()?;
            let artifact = Artifact(RepoRootDistWasm::new(output_path.as_ref()));
            Ok(Self::Watcher { artifact, watch_process })
        }
        .boxed()
    }
}



#[derive(Clone, Debug, Display)]
pub struct Artifact(RepoRootDistWasm);

impl Artifact {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self(RepoRootDistWasm::new(path))
    }
    pub fn wasm(&self) -> &Path {
        &self.0.wasm_main
    }
    pub fn js_glue(&self) -> &Path {
        &self.0.wasm_glue
    }
    pub fn dir(&self) -> &Path {
        &self.0.path
    }
}

impl AsRef<Path> for Artifact {
    fn as_ref(&self) -> &Path {
        self.0.as_path()
    }
}

impl IsArtifact for Artifact {}

impl Wasm {
    pub async fn check(&self) -> Result {
        Cargo
            .cmd()?
            .apply(&cargo::Command::Check)
            .apply(&cargo::Options::Workspace)
            .apply(&cargo::Options::Package("enso-integration-test".into()))
            .apply(&cargo::Options::AllTargets)
            .run_ok()
            .await
    }

    pub async fn test(&self, repo_root: PathBuf, wasm: bool, native: bool) -> Result {
        async fn maybe_run<Fut: Future<Output = Result>>(
            name: &str,
            enabled: bool,
            f: impl (FnOnce() -> Fut),
        ) -> Result {
            if enabled {
                info!("Will run {name} tests.");
                f().await.context(format!("Running {name} tests."))
            } else {
                info!("Skipping {name} tests.");
                Ok(())
            }
        }

        maybe_run("native", native, async || {
            Cargo
                .cmd()?
                .current_dir(repo_root.clone())
                .apply(&cargo::Command::Test)
                .apply(&cargo::Options::Workspace)
                // Color needs to be passed to tests themselves separately.
                // See: https://github.com/rust-lang/cargo/issues/1983
                .arg("--")
                .apply(&cargo::Color::Always)
                .run_ok()
                .await
        })
        .await?;

        maybe_run("wasm", wasm, || test::test_all(repo_root.clone())).await?;
        Ok(())
    }

    pub async fn integration_test(
        &self,
        source_root: PathBuf,
        _project_manager: Option<Child>,
        headless: bool,
        additional_options: Vec<String>,
    ) -> Result {
        info!("Running Rust WASM test suite.");
        use wasm_pack::TestFlags::*;
        WasmPack
            .cmd()?
            .current_dir(source_root)
            .set_env(env::WASM_BINDGEN_TEST_TIMEOUT, &INTEGRATION_TESTS_WASM_TIMEOUT.as_secs())?
            .arg("test")
            .apply_opt(headless.then_some(&Headless))
            .apply(&Chrome)
            .arg("integration-test")
            .arg("--profile=integration-test")
            .args(additional_options)
            .run_ok()
            .await
        // PM will be automatically killed by dropping the handle.
    }
}

/// Get the size of a file after gzip compression.
pub async fn compressed_size(path: impl AsRef<Path>) -> Result<byte_unit::Byte> {
    let file = tokio::io::BufReader::new(ide_ci::fs::tokio::open(&path).await?);
    let encoded_stream = async_compression::tokio::bufread::GzipEncoder::new(file);
    ide_ci::io::read_length(encoded_stream).await.map(into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ide_ci::io::read_length;
    use ide_ci::programs::Cargo;
    use semver::VersionReq;

    #[tokio::test]
    async fn check_wasm_size() -> Result {
        let path = r"H:\NBO\ci-build\dist\wasm\ide.wasm";
        let file = tokio::io::BufReader::new(ide_ci::fs::tokio::open(&path).await?);
        let encoded_stream = async_compression::tokio::bufread::GzipEncoder::new(file);
        dbg!(read_length(encoded_stream).await?);
        Ok(())
    }

    #[tokio::test]
    async fn check_wasm_pack_version() -> Result {
        WasmPack.require_present_that(&VersionReq::parse(WASM_PACK_VERSION_REQ)?).await?;
        Ok(())
    }

    #[tokio::test]
    async fn build() -> Result {
        Ok(())
    }

    #[tokio::test]
    async fn watch_by_cargo_watch() -> Result {
        pretty_env_logger::init();
        Cargo
            .cmd()?
            .arg("watch")
            .args(["--ignore", "README.md"])
            .arg("--")
            .args(["enso-build2"])
            .args(["--repo-path", r"H:\NBO\enso5"])
            .arg("wasm")
            .run_ok()
            .await?;

        Ok(())
    }
}
