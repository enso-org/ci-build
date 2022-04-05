use crate::prelude::*;
use anyhow::Context;

use crate::project::wasm::js_patcher::patch_js_glue_in_place;
// use crate::paths::generated::Parameters;
// use crate::paths::generated::Paths;
// use crate::paths::generated::PathsRepoRootDistWasm;

use crate::paths::generated::RepoRoot;
use crate::paths::generated::RepoRootDistWasm;
use crate::project::IsArtifact;
use crate::project::IsTarget;
use ide_ci::env::Variable;
use ide_ci::programs::Cargo;
use tokio::process::Child;

pub mod js_patcher;
pub mod test;


pub const WASM_ARTIFACT_NAME: &str = "gui_wasm";
pub const OUTPUT_NAME: &str = "ide";
pub const TARGET_CRATE: &str = "app/gui";

#[derive(Clone, Debug)]
pub struct BuildInput {
    pub repo_root:  RepoRoot,
    /// Path to the crate to be compiled to WAM. Relative to the repository root.
    pub crate_path: PathBuf,
}


#[derive(Clone, Debug, PartialEq)]
pub struct Wasm;

#[async_trait]
impl IsTarget for Wasm {
    type BuildInput = BuildInput;
    type Output = Artifacts;

    fn artifact_name(&self) -> &str {
        WASM_ARTIFACT_NAME
    }

    fn build(
        &self,
        input: Self::BuildInput,
        output_path: impl AsRef<Path> + Send + Sync + 'static,
    ) -> BoxFuture<'static, Result<Self::Output>> {
        // TODO:
        //   Old script intentionally built everything into temp directory first.
        //   To be checked if this was actually useful for something.
        async move {
            info!("Building wasm.");
            ide_ci::fs::create_dir_if_missing(&output_path)?;
            ide_ci::programs::WasmPack
                .cmd()?
                .kill_on_drop(true)
                .env_remove(ide_ci::programs::rustup::env::Toolchain::NAME)
                .args([
                    "-vv",
                    "build",
                    "--target",
                    "web",
                    "--out-dir",
                    output_path.as_str(),
                    "--out-name",
                    OUTPUT_NAME,
                    input.crate_path.as_str(),
                ])
                .current_dir(&input.repo_root)
                .run_ok()
                .await?;

            let ret = RepoRootDistWasm::new(output_path.as_ref());
            patch_js_glue_in_place(&ret.wasm_glue)?;
            ide_ci::fs::rename(&ret.wasm_main_raw, &ret.wasm_main)?;
            let ret = Artifacts(ret);
            Ok(ret)
        }
        .boxed()
    }
}



#[derive(Clone, Debug, Display)]
pub struct Artifacts(RepoRootDistWasm);

impl Artifacts {
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

impl AsRef<Path> for Artifacts {
    fn as_ref(&self) -> &Path {
        self.0.as_path()
    }
}

impl IsArtifact for Artifacts {
    fn from_existing(path: impl AsRef<Path>) -> BoxFuture<'static, Result<Self>> {
        ready(Ok(Artifacts::new(path.as_ref()))).boxed()
    }
}

impl Wasm {
    pub fn watch(&self, input: BuildInput, output_path: PathBuf) -> Result<Watcher> {
        // TODO
        // This is not nice, as this module should not be aware of the CLI parsing/generation.
        // Rather than using `cargo watch` this should be implemented directly in Rust.
        let child = Cargo
            .cmd()?
            .arg("watch")
            .args(["--ignore", "README.md"])
            .arg("--")
            .args(["enso-build2"])
            .args(["--repo-path", input.repo_root.as_str()])
            // FIXME crate name
            .arg("wasm")
            .arg("build")
            .args(["--wasm-output-path", output_path.as_str()])
            .spawn()?;
        let ret = RepoRootDistWasm::new(output_path);
        Ok(Watcher { artifacts: Artifacts(ret), watch_process: child })
    }

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

pub struct Watcher {
    /// Where the watcher outputs artifacts.
    pub artifacts:     Artifacts,
    /// The process performing the watch.
    ///
    /// In this case, an instance of cargo-watch.
    pub watch_process: Child,
}

pub enum Watched {
    Watcher(Watcher),
    Static(Artifacts),
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
