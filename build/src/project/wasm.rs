use crate::prelude::*;
use anyhow::Context;
use std::env::temp_dir;
use std::fs::Metadata;
use tokio::process::Child;

use crate::project::wasm::js_patcher::patch_js_glue;
use crate::project::wasm::js_patcher::patch_js_glue_in_place;
// use crate::paths::generated::Parameters;
// use crate::paths::generated::Paths;
// use crate::paths::generated::PathsRepoRootDistWasm;

use crate::paths::generated::RepoRoot;
use crate::paths::generated::RepoRootDistWasm;
use crate::project::IsArtifact;
use crate::project::IsTarget;
use crate::project::IsWatchable;
use ide_ci::env::Variable;
use ide_ci::programs::wasm_pack::Target;
use ide_ci::programs::Cargo;

pub mod js_patcher;
pub mod test;

pub mod env {
    // Enable a Rust unstable feature that the `#[profile]` macro uses to obtain source-file and
    // line number information to include in generated profile files.
    //
    // The IntelliJ Rust plugin does not support the `proc_macro_span` Rust feature; using it causes
    // JetBrains IDEs to become entirely unaware of the items produced by `#[profile]`.
    // (See: https://github.com/intellij-rust/intellij-rust/issues/8655)
    //
    // In order to have line number information in actual usage, but keep everything understandable
    // by JetBrains IDEs, we need IntelliJ/CLion to build crates differently from how they are
    // built for the application to be run. This is accomplished by gating the use of the unstable
    // functionality by a `cfg` flag. A `cfg` flag is disabled by default, so when a Rust IDE builds
    // crates internally in order to determine macro expansions, it will do so without line numbers.
    // When this script is used to build the application, it is not for the purpose of IDE macro
    // expansion, so we can safely enable line numbers.
    //
    // The reason we don't use a Cargo feature for this is because this script can build different
    // crates, and we'd like to enable this feature when building any crate that depends on the
    // `profiler` crates. We cannot do something like '--feature=enso_profiler/line-numbers' without
    // causing build to fail when building a crate that doesn't have `enso_profiler` in its
    // dependency tree.
    ide_ci::define_env_var!(ENSO_ENABLE_PROC_MACRO_SPAN, bool);
}

pub const WASM_ARTIFACT_NAME: &str = "gui_wasm";
pub const OUTPUT_NAME: &str = "ide";
pub const TARGET_CRATE: &str = "app/gui";

#[derive(Clone, Debug)]
pub struct BuildInput {
    pub repo_root:           RepoRoot,
    /// Path to the crate to be compiled to WAM. Relative to the repository root.
    pub crate_path:          PathBuf,
    pub extra_cargo_options: Vec<String>,
}


#[derive(Clone, Debug, PartialEq)]
pub struct Wasm;

#[async_trait]
impl IsTarget for Wasm {
    type BuildInput = BuildInput;
    type Artifact = Artifact;

    fn artifact_name(&self) -> &str {
        WASM_ARTIFACT_NAME
    }

    fn build(
        &self,
        input: Self::BuildInput,
        output_path: impl AsRef<Path> + Send + Sync + 'static,
    ) -> BoxFuture<'static, Result<Self::Artifact>> {
        // TODO:
        //   Old script intentionally built everything into temp directory first.
        //   To be checked if this was actually useful for something.
        let span = info_span!("Building WASM.",
            repo = %input.repo_root.display(),
            crate = %input.crate_path.display(),
            cargo_opts = ?input.extra_cargo_options
        );
        async move {
            info!("Building wasm.");
            let temp_dir = temp_dir();
            let temp_dist = RepoRootDistWasm::new(temp_dir.as_path());
            ide_ci::programs::WasmPack
                .cmd()?
                .current_dir(&input.repo_root)
                .kill_on_drop(true)
                .env_remove(ide_ci::programs::rustup::env::Toolchain::NAME)
                .set_env(env::ENSO_ENABLE_PROC_MACRO_SPAN, &true)?
                .build()
                .target(Target::Web)
                .output_directory(&temp_dist)
                .output_name(&OUTPUT_NAME)
                .arg(&input.crate_path)
                .arg("--")
                .arg("--color=always")
                .args(&input.extra_cargo_options)
                .run_ok()
                .await?;
            patch_js_glue_in_place(&temp_dist.wasm_glue)?;

            ide_ci::fs::create_dir_if_missing(&output_path)?;
            let ret = RepoRootDistWasm::new(output_path.as_ref());
            copy_if_different(&temp_dist.wasm_glue, &ret.wasm_glue)?;
            copy_if_different(&temp_dist.wasm_main_raw, &ret.wasm_main)?;
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
            let watch_process = Cargo
                .cmd()?
                .kill_on_drop(true)
                .current_dir(&input.repo_root)
                .arg("watch")
                .args(["--ignore", "README.md"])
                .arg("--")
                .args(["enso-build2"])
                .args(["--repo-path", input.repo_root.as_str()])
                // FIXME crate name
                .arg("wasm")
                .arg("build")
                .args(["--crate-path", input.crate_path.as_str()])
                .args(["--wasm-output-path", output_path.as_str()])
                .spawn_intercepting()?;
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

impl IsArtifact for Artifact {
    fn from_existing(path: impl AsRef<Path>) -> BoxFuture<'static, Result<Self>> {
        ready(Ok(Artifact::new(path.as_ref()))).boxed()
    }
}

impl Wasm {
    pub async fn check(&self) -> Result {
        Cargo
            .cmd()?
            .args(["check", "--workspace", "-p", "enso-integration-test", "--all-targets"])
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
                .arg("test")
                .arg("--workspace")
                .run_ok()
                .await
        })
        .await?;

        maybe_run("wasm", wasm, || test::test_all(repo_root.clone())).await?;
        // let native_job = async move  {
        //     if native {
        //         info!("Will run native tests.");
        //         Cargo.cmd()?.arg("test").arg("--workspace").run_ok().await
        //     } else {
        //         info!("Skipping native tests.");
        //         Ok(())
        //     }
        // };
        // let wasm_job = async move  {
        //     if wasm {
        //         info!("Will run WASM tests.");
        //         Cargo.cmd()?.arg("test").arg("--workspace").run_ok().await
        //     } else {
        //         info!("Skipping WASM tests.");
        //         Ok(())
        //     }
        // };
        // let wasm_job = Cargo
        //     .cmd()?
        //     .arg("run")
        //     .args(["--manifest-path", "build/rust-scripts/Cargo.toml"])
        //     .args(["--bin", "test_all"])
        //     .arg("--")
        //     .arg("--headless")
        //     .arg("--chrome")
        //     .env("WASM_BINDGEN_TEST_TIMEOUT", "60")
        //     .run_ok();

        // if (argv.native) {
        //     console.log(`Running Rust test suite.`)
        //     await run_cargo('cargo', ['test', '--workspace'])
        // }
        //
        // if (argv.wasm) {
        //     console.log(`Running Rust WASM test suite.`)
        //     process.env.WASM_BINDGEN_TEST_TIMEOUT = 60
        //     await run_cargo('cargo', args)
        // }
        Ok(())
    }
}

// #[derive(Clone, Debug)]
// pub enum WasmSource {
//     Build { repo_root: PathBuf },
//     Local(PathBuf),
//     GuiCiRun { repo: RepoContext, run: RunId },
// }
//
// impl WasmSource {
//     pub async fn place_at(
//         &self,
//         client: &Octocrab,
//         output_dir: &RepoRootDistWasm,
//     ) -> Result<Artifacts> {
//         match self {
//             WasmSource::Build { repo_root } => {
//                 build_wasm(repo_root, output_dir).await?;
//             }
//             WasmSource::Local(local_path) => {
//                 ide_ci::fs::copy(local_path, output_dir)?;
//             }
//             WasmSource::GuiCiRun { repo, run } => {
//                 download_wasm_from_run(client, &repo, *run, output_dir).await?;
//             }
//         }
//         Ok(Artifacts::new(output_dir))
//     }
// }
//
// // "Failed to find artifacts for run {run} in {repo}."
// pub async fn download_wasm_from_run(
//     client: &Octocrab,
//     repo: &RepoContext,
//     run: RunId,
//     output_path: impl AsRef<Path>,
// ) -> Result {
//     let artifacts = client
//         .actions()
//         .list_workflow_run_artifacts(&repo.owner, &repo.name, run)
//         .per_page(100)
//         .send()
//         .await?
//         .value
//         .context(format!("Failed to find any artifacts."))?;
//
//     let wasm_artifact = artifacts
//         .into_iter()
//         .find(|artifact| artifact.name == WASM_ARTIFACT_NAME)
//         .context(format!("Failed to find artifact by name {WASM_ARTIFACT_NAME}"))?;
//
//     let wasm = client
//         .actions()
//         .download_artifact(&repo.owner, &repo.name, wasm_artifact.id, ArchiveFormat::Zip)
//         .await?;
//     let wasm = std::io::Cursor::new(wasm);
//     let mut wasm = zip::ZipArchive::new(wasm)?;
//
//     ide_ci::fs::create_dir_if_missing(&output_path)?;
//     wasm.extract(&output_path)?;
//     Ok(())
// }

#[cfg(test)]
mod tests {
    use super::*;
    use ide_ci::programs::Cargo;

    // pub struct WasmWatcher {
    //     /// Drop this field to stop the event generation job.
    //     _tx:                 tokio::sync::watch::Sender<WorkingData>,
    //     pub ongoing_build:   Arc<Mutex<Option<JoinHandle<Result<Artifacts>>>>>,
    //     pub event_generator: JoinHandle<std::result::Result<(), CriticalError>>,
    // }
    // impl WasmWatcher {
    //     pub fn new(input: RepoRoot, output_path: PathBuf) -> Result<Self> {
    //         let mut initial_config = WorkingData::default();
    //         initial_config.pathset = vec![input.path.clone().into()];
    //
    //         let (tx, rx) = tokio::sync::watch::channel(default());
    //         // We cannot just use initial value when creating the channel, as the watchexec
    // expects         // at least one change.
    //         tx.send(initial_config)?;
    //         let (errors_tx, errors_rx) = tokio::sync::mpsc::channel(1024);
    //         let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(1024);
    //         let event_generator = watchexec::fs::worker(rx, errors_tx, event_tx);
    //
    //         let event_generator = tokio::spawn(event_generator);
    //         // let ok_stream = to_stream(event_rx).map(Result::Ok);
    //         // let err_stream = to_stream(errors_rx).map(|e: RuntimeError|
    // Result::Err(e.into()));         // let mut stream = futures::stream::select(ok_stream,
    // err_stream);
    //
    //         let mut ongoing_build = Arc::new(Mutex::new(None));
    //         let ret = Self { _tx: tx, ongoing_build, event_generator };
    //         let ongoing_build = ret.ongoing_build.clone();
    //         let event_processor = async move {
    //             debug!("Awaiting events");
    //             //while let Some(msg) = stream.next().await {
    //             while let Some(msg) = event_rx.recv().await {
    //                 trace!("Received a new event: {msg:#?}.");
    //                 let previous_run = ongoing_build.lock().await.take();
    //                 if let Some(previous_run) = previous_run {
    //                     info!("Aborting previous WASM build.");
    //                     previous_run.abort();
    //                     drop(previous_run);
    //                     // info!("Waiting for previous run to end.");
    //                     // let result = previous_run.await?;
    //                     // if let Err(e) = result {
    //                     //     warn!("Previous run failed: {e}");
    //                     // } else {
    //                     //     warn!("Previous run completed.")
    //                     // }
    //                 }
    //                 trace!("Spawning a new build job.");
    //                 let new_run = tokio::spawn(Wasm.build(input.clone(), output_path.clone()));
    //                 *ongoing_build.lock().await = Some(new_run);
    //             }
    //             debug!("Finished event processing.");
    //             Result::Ok(())
    //         };
    //
    //         tokio::spawn(event_processor);
    //         Ok(ret)
    //     }
    // }
    // #[tokio::test]
    // async fn watcher() -> Result {
    //     // console_subscriber::init();
    //     pretty_env_logger::init();
    //     debug!("Test is starting!");
    //     let repo_root =
    //         RepoRoot::new(r"H:\NBO\enso5", TargetTriple::new(Versions::default()).to_string());
    //     let _w = WasmWatcher::new(repo_root.clone(), repo_root.dist.wasm.into())?;
    //     tokio::time::sleep(Duration::from_secs(60 * 5)).await;
    //     // let mut initial_config = WorkingData::default();
    //     // initial_config.pathset = vec![r"H:\NBO\enso5".into()];
    //     // let (tx, rx) = tokio::sync::watch::channel(initial_config.clone());
    //     // tx.send(initial_config)?;
    //     // let (errors_tx, errors_rx) = tokio::sync::mpsc::channel(1024);
    //     // let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(1024);
    //     //
    //     // let l1 = async move {
    //     //     debug!("Awaiting events");
    //     //     while let Some(msg) = event_rx.recv().await {
    //     //         dbg!(&msg);
    //     //         warn!("{msg:#?}");
    //     //     }
    //     // };
    //     //
    //     // tokio::spawn(l1);
    //     // let worker = watchexec::fs::worker(rx, errors_tx, event_tx);
    //     // worker.await?;
    //     Ok(())
    // }

    #[tokio::test]
    async fn build() -> Result {
        Ok(())
    }
    //
    // #[tokio::test]
    // async fn test_artifact_download() -> Result {
    //     let out = r"C:\temp\wasm";
    //     let client = setup_octocrab()?;
    //     // let client = OctocrabBuilder::new()
    //     //     .personal_token("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".into())
    //     //     .build()?;
    //     let repo = RepoContext { owner: "enso-org".into(), name: "enso".into() };
    //     // https://github.com/enso-org/enso/actions/runs/1982165517
    //     download_wasm_from_run(&client, &repo, RunId(1982165517), out).await?;
    //     Ok(())
    // }

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


        // 'build',
        // '--target',
        // 'web',
        // '--out-dir',
        // paths.wasm.root,
        // '--out-name',
        // 'ide',
        // crate,
        // ]


        Ok(())
    }
    //
    // #[tokio::test(flavor = "multi_thread")]
    // async fn watch_test() -> Result {
    //     use watchexec::action::Action;
    //     use watchexec::action::Outcome;
    //     use watchexec::config::InitConfig;
    //     use watchexec::config::RuntimeConfig;
    //     use watchexec::handler::Handler as _;
    //     use watchexec::handler::PrintDebug;
    //     use watchexec::Watchexec;
    //
    //     let repo_root = "H:/NBO/enso5";
    //
    //     let mut init = InitConfig::default();
    //     init.on_error(PrintDebug(std::io::stderr()));
    //
    //     let mut runtime = RuntimeConfig::default();
    //     runtime.pathset([repo_root]);
    //     runtime.action_throttle(Duration::from_millis(500));
    //     runtime.command(["cargo"]);
    //     runtime.command_shell(watchexec::command::Shell::None);
    //
    //     runtime.on_pre_spawn(move |prespawn: PreSpawn| {
    //         println!("======================================");
    //         dbg!(prespawn);
    //         println!("*****************************************");
    //         ready(std::result::Result::<_, Infallible>::Ok(()))
    //     });
    //
    //     runtime.on_action(async move |action: Action| -> std::result::Result<_, Infallible> {
    //         // let ret = ready(std::result::Result::<_, Infallible>::Ok(()));
    //
    //         println!("Action!");
    //         dbg!(&action);
    //
    //         let signals = action.events.iter().flat_map(Event::signals).collect_vec();
    //         let paths = action.events.iter().flat_map(Event::paths).collect_vec();
    //         let got_stop_signal = signals
    //             .iter()
    //             .any(|signal| matches!(signal, MainSignal::Terminate | MainSignal::Interrupt));
    //
    //         if got_stop_signal {
    //             return Ok(action.outcome(Outcome::both(Outcome::Stop, Outcome::Exit)));
    //         }
    //         if paths.is_empty() && !signals.is_empty() {
    //             let outcome = signals.iter().copied().fold(Outcome::DoNothing, |acc, signal| {
    //                 Outcome::both(acc, Outcome::Signal(signal.into()))
    //             });
    //             return Ok(action.outcome(outcome));
    //         }
    //         if paths.is_empty() {
    //             let completion = action.events.iter().flat_map(Event::completions).next();
    //             if let Some(status) = completion {
    //                 info!("Command completed with status: {:?}", status);
    //             }
    //             return Ok(action.outcome(Outcome::DoNothing));
    //         }
    //
    //         let when_running = Outcome::both(Outcome::Stop, Outcome::Start);
    //         let when_idle = Outcome::Start;
    //         Ok(action.outcome(Outcome::if_running(when_running, when_idle)))
    //     });
    //
    //     let we = Watchexec::new(init, runtime.clone())?;
    //     let c = runtime.clone();
    //     we.send_event(Event::default()).await?;
    //     println!("Starting.");
    //     we.main().await?;
    //     Ok(())
    // }
}
