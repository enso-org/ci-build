use crate::prelude::*;

pub mod context;
pub mod desktop;
pub mod pm_provider;
pub mod wasm;
pub mod web;


#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildInfo {
    pub commit:         String,
    pub version:        Version,
    pub engine_version: Version,
    pub name:           String,
}

//
// #[tokio::main]
// async fn main() -> Result {
//     let root_path =
//         PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| String::from(r"H:/NBO/enso5")))
//             .absolutize()?
//             .to_path_buf();
//     let temp = tempfile::tempdir()?;
//
//     let octocrab = setup_octocrab()?;
//     let build_kind = BuildKind::Dev;
//     let repo = None;
//
//     let versions =
//         enso_build::version::deduce_versions(&octocrab, build_kind, repo, &root_path).await?;
//     let triple = TargetTriple::new(versions);
//     triple.versions.publish()?;
//
//     //let temp = temp.path();
//     let params = enso_build::paths::generated::Parameters {
//         repo_root: root_path.clone(),
//         temp:      temp.path().to_owned(),
//         triple:    triple.to_string().into(),
//     };
//
//     dbg!(&params);
//     let paths = enso_build::paths::generated::Paths::new(&params, &PathBuf::from("."));
//     std::env::set_var("ENSO_IDE_DIST", &paths.repo_root.dist.path);
//     // let versions = Versions::new(Version::parse("2022.1.1-nightly.2022-02-03")?);
//     // versions.publish()?;
//
//     let info_for_js = BuildInfo {
//         commit:         "badf00d".into(),
//         name:           "Enso IDE".into(),
//         version:        triple.versions.version.clone(),
//         engine_version: triple.versions.version.clone(),
//     };
//
//     ide_ci::fs::write(
//         &paths.repo_root.app.ide_desktop.join("build.json"),
//         serde_json::to_string(&info_for_js)?,
//     )?;
//
//     //
//     if WasmPack.require_present().await.is_err() {
//         Cargo.cmd()?.args(["install", "wasm-pack"]).run_ok().await?;
//     }
//     //
//
//
//     if true {
//         let pm_source = ProjectManagerSource::Local;
//         // let pm_source = ProjectManagerSource::Bundle(
//         //
// paths.repo_root.built_distribution.project_manager_bundle_triple.enso.path.clone(),         // );
//
//         let get_pm_fut = {
//             let paths = paths.clone();
//             let triple = triple.clone();
//             async move { pm_source.get(paths, triple).await }
//         }
//         .boxed();
//
//         let get_wasm_fut = {
//             let paths = paths.clone();
//             async move { build_wasm(&paths).await }.boxed()
//         };
//
//         let parallelism = match is_self_hosted() {
//             Ok(false) => Sequential,
//             _ => FutureParallelism,
//         };
//         let _res = try_join_all([get_pm_fut, get_wasm_fut], parallelism).await?;
//     }
//     // ProjectManagerSource::Local.get(&paths, &triple).await?;
//     // build_wasm(&paths).await?;
//     // if (!argv.dev) {
//     //     console.log('Minimizing the WASM binary.')
//     //                 await gzip(paths.wasm.main, paths.wasm.mainGz)
//     //
//     //     const limitMb = 4.6
//     //     await checkWasmSize(paths.wasm.mainGz, limitMb)
//     // }
//     // Copy WASM files from temporary directory to Webpack's `dist` directory.
//     // ide_ci::io::copy(paths.wasm(), paths.dist_wasm())?;
//
//     // let pm = tokio::task::spawn(get_pm);
//     // let wasm = tokio::task::spawn(build_wasm(&paths));
//     // let a = try_join_all([pm, wasm], TaskParallelism).await?;
//
//     // // JS PART
//
//     // Lerna.cmd()?.arg("bootstrap").current_dir(&paths.repo_root.app.ide_desktop).run_ok().
// await?;     // Npm.args(["run",
// "install"])?.current_dir(&paths.repo_root.app.ide_desktop).run_ok().await?;
//
//
//
//     download_js_assets(&paths.repo_root.app.ide_desktop.lib.content).await?;
//
//     Npm.install(&paths.repo_root.app.ide_desktop)?.run_ok().await?;
//
//     Npm.cmd()?
//         .args(["--workspace", "enso-studio-icons"])
//         .args(["run", "build"])
//         .arg(&paths.repo_root.dist.icons)
//         .current_dir(&paths.repo_root.app.ide_desktop)
//         .run_ok()
//         .await?;
//
//     Npm.cmd()?
//         .args(["--workspace", "enso-studio-content"])
//         .args(["run", "build"])
//         .current_dir(&paths.repo_root.app.ide_desktop)
//         .run_ok()
//         .await?;
//
//     Npm.cmd()?
//         .args(["--workspace", "enso"])
//         .args(["run", "build"])
//         .current_dir(&paths.repo_root.app.ide_desktop)
//         .run_ok()
//         .await?;
//
//     Npm.cmd()?
//         .args(["--workspace", "enso"])
//         .args(["run", "dist"]) // , "--", "--win", "dir"
//         .current_dir(&paths.repo_root.app.ide_desktop)
//         .run_ok()
//         .await?;
//
//
//     println!("{}", paths.temp.display());
//     std::mem::forget(paths.temp);
//
//     // Command::new(r"H:\NBO\enso5\dist\client\win-unpacked\Enso.exe").run_ok().await?;
//
//     // Npm.cmd()?.current_dir(&paths.repo_root.app.ide_desktop).args(["run",
//     // "dist"]).run_ok().await?;
//
//     Ok(())
// }
