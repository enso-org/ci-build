//! Wrappers over the Rust part of the IDE codebase.

use crate::prelude::*;

use crate::paths::generated::RepoRoot;
use crate::paths::generated::RepoRootDistWasm;
use crate::project::wasm::js_patcher::patch_js_glue_in_place;
use crate::project::Context;
use crate::project::IsArtifact;
use crate::project::IsTarget;
use crate::project::IsWatchable;
use crate::source::BuildTargetJob;
use crate::source::WatchTargetJob;
use crate::source::WithDestination;
use derivative::Derivative;
use ide_ci::cache;
use ide_ci::env::Variable;
use ide_ci::fs::compressed_size;
use ide_ci::fs::copy_file_if_different;
use ide_ci::programs::cargo;
use ide_ci::programs::wasm_opt;
use ide_ci::programs::wasm_opt::WasmOpt;
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

pub const BINARYEN_VERSION_TO_INSTALL: u32 = 108;

pub const DEFAULT_INTEGRATION_TESTS_WASM_TIMEOUT: Duration = Duration::from_secs(300);

pub const INTEGRATION_TESTS_CRATE_NAME: &str = "enso-integration-test";

pub const OUTPUT_NAME: &str = "ide";

/// Name of the artifact that will be uploaded as part of CI run.
pub const WASM_ARTIFACT_NAME: &str = "gui_wasm";

pub const DEFAULT_TARGET_CRATE: &str = "app/gui";

#[derive(Clone, Copy, Debug, Default, strum::Display, strum::EnumString, PartialEq)]
#[strum(serialize_all = "kebab-case")]
pub enum ProfilingLevel {
    #[default]
    Objective,
    Task,
    Detail,
    Debug,
}

#[derive(clap::ArgEnum, Clone, Copy, Debug, PartialEq, strum::Display, strum::AsRefStr)]
#[strum(serialize_all = "kebab-case")]
pub enum Profile {
    Dev,
    Profile,
    Release,
    // Production,
}

impl From<Profile> for wasm_pack::Profile {
    fn from(profile: Profile) -> Self {
        match profile {
            Profile::Dev => Self::Dev,
            Profile::Profile => Self::Profile,
            Profile::Release => Self::Release,
            // Profile::Production => Self::Release,
        }
    }
}

impl Profile {
    pub fn should_check_size(self) -> bool {
        match self {
            Profile::Dev => false,
            Profile::Profile => false,
            Profile::Release => true,
            // Profile::Production => true,
        }
    }

    pub fn extra_rust_options(self) -> Vec<String> {
        match self {
            // Profile::Production => ["-Clto=fat", "-Ccodegen-units=1", "-Cincremental=false"]
            //     .into_iter()
            //     .map(ToString::to_string)
            //     .collect(),
            Profile::Dev | Profile::Profile | Profile::Release => vec![],
        }
    }

    pub fn optimization_level(self) -> wasm_opt::OptimizationLevel {
        match self {
            Profile::Dev => wasm_opt::OptimizationLevel::O0,
            Profile::Profile => wasm_opt::OptimizationLevel::O,
            Profile::Release => wasm_opt::OptimizationLevel::O3,
        }
    }
}

#[derive(Clone, Derivative)]
#[derivative(Debug)]
pub struct BuildInput {
    #[derivative(Debug(format_with = "std::fmt::Display::fmt"))]
    pub repo_root:           RepoRoot,
    /// Path to the crate to be compiled to WAM. Relative to the repository root.
    pub crate_path:          PathBuf,
    pub wasm_opt_options:    Vec<String>,
    pub skip_wasm_opt:       bool,
    pub extra_cargo_options: Vec<String>,
    pub profile:             Profile,
    pub profiling_level:     Option<ProfilingLevel>,
    pub wasm_size_limit:     Option<byte_unit::Byte>,
}

impl BuildInput {
    pub async fn perhaps_check_size(&self, wasm_path: impl AsRef<Path>) -> Result {
        let compressed_size = compressed_size(&wasm_path).await?.get_appropriate_unit(true);
        info!("Compressed size of {} is {}.", wasm_path.as_ref().display(), compressed_size);
        if let Some(wasm_size_limit) = self.wasm_size_limit {
            let wasm_size_limit = wasm_size_limit.get_appropriate_unit(true);
            if !self.profile.should_check_size() {
                warn!("Skipping size check because profile is '{}'.", self.profile,);
            } else if self.profiling_level.unwrap_or_default() != ProfilingLevel::Objective {
                // TODO? additional leeway as sanity check
                warn!(
                    "Skipping size check because profiling level is {:?} rather than {}.",
                    self.profiling_level,
                    ProfilingLevel::Objective
                );
            } else {
                ensure!(
                    compressed_size < wasm_size_limit,
                    "Compressed WASM size {} exceeds the limit of {}.",
                    compressed_size,
                    wasm_size_limit
                )
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

    fn build_internal(
        &self,
        context: Context,
        job: BuildTargetJob<Self>,
    ) -> BoxFuture<'static, Result<Self::Artifact>> {
        let Context { octocrab: _, cache, upload_artifacts: _ } = context;
        let WithDestination { inner, destination } = job;
        let span = info_span!("Building WASM.",
            repo = %inner.repo_root.display(),
            crate = %inner.crate_path.display(),
            cargo_opts = ?inner.extra_cargo_options
        );
        async move {
            // Old wasm-pack does not pass trailing `build` command arguments to the Cargo.
            // We want to be able to pass --profile this way.
            WasmPack.require_present_that(VersionReq::parse(">=0.10.1")?).await?;

            let BuildInput {
                repo_root,
                crate_path,
                wasm_opt_options,
                skip_wasm_opt,
                extra_cargo_options,
                profile,
                profiling_level,
                wasm_size_limit: _wasm_size_limit,
            } = &inner;

            cache::goodie::binaryen::Binaryen { version: BINARYEN_VERSION_TO_INSTALL }
                .install_if_missing(&cache)
                .await?;

            info!("Building wasm.");
            let temp_dir = tempdir()?;
            let temp_dist = RepoRootDistWasm::new_root(temp_dir.path());
            let mut command = ide_ci::programs::WasmPack.cmd()?;
            command
                .current_dir(&repo_root)
                .kill_on_drop(true)
                .env_remove(ide_ci::programs::rustup::env::Toolchain::NAME)
                .set_env(env::ENSO_ENABLE_PROC_MACRO_SPAN, &true)?
                .build()
                .arg(&wasm_pack::Profile::from(*profile))
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

            Self::finalize_wasm(wasm_opt_options, *skip_wasm_opt, *profile, &temp_dist).await?;
            patch_js_glue_in_place(&temp_dist.wasm_glue)?;

            ide_ci::fs::create_dir_if_missing(&destination)?;
            let ret = RepoRootDistWasm::new_root(&destination);
            ide_ci::fs::copy(&temp_dist, &ret)?;
            // copy_if_different(&temp_dist, &ret).await?;
            // copy_if_different(&temp_dist.wasm_main_raw, &ret.wasm_main)?;
            inner.perhaps_check_size(&ret.wasm_main).await?;
            Ok(Artifact(ret))
        }
        .instrument(span)
        .boxed()
    }
}

#[derive(Clone, Derivative)]
#[derivative(Debug)]
pub struct WatchInput {
    pub cargo_watch_options: Vec<String>,
}

impl IsWatchable for Wasm {
    type Watcher = crate::project::Watcher<Self, Child>;
    type WatchInput = WatchInput;

    fn watch(
        &self,
        _context: Context,
        job: WatchTargetJob<Self>,
    ) -> BoxFuture<'static, Result<Self::Watcher>> {
        async move {
            let WatchTargetJob {
                watch_input: WatchInput { cargo_watch_options: cargo_watch_flags },
                build: WithDestination { inner, destination },
            } = job;
            let BuildInput {
                repo_root,
                crate_path,
                wasm_opt_options,
                skip_wasm_opt,
                extra_cargo_options,
                profile,
                profiling_level,
                wasm_size_limit,
            } = inner;

            let current_exe = std::env::current_exe()?;
            // Cargo watch apparently cannot handle verbatim path prefix. We remove it and hope for
            // the best.
            let current_exe = current_exe.without_verbatim_prefix();


            let mut watch_cmd = Cargo.cmd()?;

            watch_cmd
                .kill_on_drop(true)
                .current_dir(&repo_root)
                .arg("watch")
                .args(["--ignore", "README.md"])
                .args(cargo_watch_flags)
                .arg("--")
                // TODO [mwu]
                // This is not nice, as this module should not be aware of the CLI
                // parsing/generation. Rather than using `cargo watch` this should
                // be implemented directly in Rust.
                .arg(current_exe)
                .arg("--skip-version-check") // We already checked in the parent process.
                .args(["--repo-path", repo_root.as_str()])
                .arg("wasm")
                .arg("build")
                .args(["--crate-path", crate_path.as_str()])
                .args(["--wasm-output-path", destination.as_str()])
                .args(["--wasm-profile", profile.as_ref()]);
            if let Some(profiling_level) = profiling_level {
                watch_cmd.args(["--profiling-level", profiling_level.to_string().as_str()]);
            }
            for wasm_opt_option in wasm_opt_options {
                watch_cmd.args(["--wasm-opt-option", &wasm_opt_option]);
            }
            if skip_wasm_opt {
                watch_cmd.args(["--skip-wasm-opt"]);
            }
            if let Some(wasm_size_limit) = wasm_size_limit {
                watch_cmd.args(["--wasm-size-limit", wasm_size_limit.to_string().as_str()]);
            }
            watch_cmd.arg("--").args(extra_cargo_options);

            let watch_process = watch_cmd.spawn_intercepting()?;
            let artifact = Artifact(RepoRootDistWasm::new_root(&destination));
            Ok(Self::Watcher { artifact, watch_process })
        }
        .boxed()
    }
}



#[derive(Clone, Debug, Display)]
pub struct Artifact(RepoRootDistWasm);

impl Artifact {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self(RepoRootDistWasm::new_root(path))
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
            .apply(&cargo::Options::Package(INTEGRATION_TESTS_CRATE_NAME.into()))
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
        wasm_timeout: Option<Duration>,
    ) -> Result {
        info!("Running Rust WASM test suite.");
        use wasm_pack::TestFlags::*;
        WasmPack
            .cmd()?
            .current_dir(source_root)
            .set_env_opt(
                env::WASM_BINDGEN_TEST_TIMEOUT,
                wasm_timeout.map(|d| d.as_secs()).as_ref(),
            )?
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

    /// Process "raw" WASM (as compiled) by optionally invoking wasm-opt.
    pub async fn finalize_wasm(
        wasm_opt_options: &[String],
        skip_wasm_opt: bool,
        profile: Profile,
        temp_dist: &RepoRootDistWasm,
    ) -> Result {
        let should_call_wasm_opt = {
            if profile == Profile::Dev {
                debug!("Skipping wasm-opt invocation, as it is not part of profile {profile}.");
                false
            } else if skip_wasm_opt {
                debug!("Skipping wasm-opt invocation, as it was explicitly requested.");
                false
            } else {
                true
            }
        };

        if should_call_wasm_opt {
            let mut wasm_opt_command = WasmOpt.cmd()?;
            let has_custom_opt_level = wasm_opt_options.iter().any(|opt| {
                wasm_opt::OptimizationLevel::from_str(opt.trim_start_matches('-')).is_ok()
            });
            if !has_custom_opt_level {
                wasm_opt_command.apply(&profile.optimization_level());
            }
            wasm_opt_command
                .args(wasm_opt_options)
                .arg(&temp_dist.wasm_main_raw)
                .apply(&wasm_opt::Output(&temp_dist.wasm_main))
                .run_ok()
                .await?;
        } else {
            copy_file_if_different(&temp_dist.wasm_main_raw, &temp_dist.wasm_main)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ide_ci::io::read_length;
    use ide_ci::programs::Cargo;

    #[tokio::test]
    async fn check_wasm_size() -> Result {
        let path = r"H:\NBO\ci-build\dist\wasm\ide.wasm";
        let file = tokio::io::BufReader::new(ide_ci::fs::tokio::open(&path).await?);
        let encoded_stream = async_compression::tokio::bufread::GzipEncoder::new(file);
        dbg!(read_length(encoded_stream).await?);
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
