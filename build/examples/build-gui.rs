#![feature(exit_status_error)]

use enso_build::prelude::*;

use enso_build::args::BuildKind;
use enso_build::engine::BuildConfiguration;
use enso_build::engine::BuildOperation;
use enso_build::gui::js_patcher::patch_js_glue;
use enso_build::gui::pm_provider::ProjectManagerSource;
use enso_build::paths::generated::Paths;
use enso_build::paths::root_to_changelog;
use enso_build::paths::GuiPaths;
use enso_build::paths::TargetTriple;
use enso_build::setup_octocrab;
use enso_build::version::Versions;
use ide_ci::env::Variable;
use ide_ci::future::try_join_all;
use ide_ci::future::AsyncPolicy::FutureParallelism;
use ide_ci::goodie::GoodieDatabase;
use ide_ci::io::download_all;
use ide_ci::programs::Git;
use ide_ci::programs::Npm;
use tempfile::TempDir;

async fn download_js_assets(paths: &Paths) -> Result {
    let output = paths.repo_root.app.ide_desktop.lib.content.join("assets");
    const ARCHIVED_ASSET_FILE: &str = "ide-assets-main/content/assets/";
    let archived_asset_prefix = PathBuf::from(ARCHIVED_ASSET_FILE);
    let ide_assets_url = "https://github.com/enso-org/ide-assets/archive/refs/heads/main.zip";
    let archive = download_all(ide_assets_url).await?;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(archive))?;
    ide_ci::archive::zip::extract_subtree(&mut archive, &archived_asset_prefix, &output)?;
    Ok(())
}

async fn init(paths: &Paths) -> Result {
    let init_token = &paths.repo_root.dist.build_init;
    if !init_token.exists() {
        println!("Initialization");
        println!("Installing build script dependencies.");
        Npm.cmd()?.current_dir(&paths.repo_root.build).arg("install").run_ok().await?;
        ide_ci::io::write(&init_token, "")?;
    }
    Ok(())
}


#[derive(Debug, Shrinkwrap)]
pub struct GuiPathsData {
    #[shrinkwrap(main_field)]
    pub root: PathBuf,
    pub temp: TempDir,
}

impl GuiPaths for GuiPathsData {
    fn root(&self) -> &Path {
        &self.root
    }

    fn temp(&self) -> &Path {
        self.temp.path()
    }
}


pub async fn build_wasm(paths: &Paths) -> Result {
    init(&paths).await?;

    let target_crate = "app/gui";
    let wasm_dir = &paths.repo_root.dist.wasm;

    ide_ci::programs::WasmPack
        .cmd()?
        .env_remove(ide_ci::programs::rustup::env::Toolchain::NAME)
        .args([
            "-vv",
            "build",
            "--target",
            "web",
            "--out-dir",
            wasm_dir.as_str(), // &paths.wasm().as_os_str().to_str().unwrap(),
            "--out-name",
            "ide",
            target_crate,
        ])
        .current_dir(&paths.repo_root)
        .spawn()?
        .wait()
        .await?
        .exit_ok()?;

    let glue_path = &wasm_dir.wasm_glue;
    patch_js_glue(std::fs::read_to_string(&glue_path)?, &glue_path)?;
    std::fs::rename(&wasm_dir.wasm_main_raw, &wasm_dir.wasm_main)?;
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BuildInfo {
    pub commit:  String,
    pub version: Version,
    pub name:    String,
}

#[tokio::main]
async fn main() -> Result {
    let root_path =
        PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| String::from(r"H:/NBO/enso5")));
    let temp = tempfile::tempdir()?;

    let octocrab = setup_octocrab()?;
    let build_kind = BuildKind::Dev;
    let repo = None;

    let versions =
        enso_build::version::deduce_versions(&octocrab, build_kind, repo, &root_path).await?;
    let triple = TargetTriple::new(versions);
    triple.versions.publish()?;

    //let temp = temp.path();
    let params = enso_build::paths::generated::Parameters {
        repo_root: root_path.clone(),
        temp:      temp.path().to_owned(),
        triple:    triple.to_string().into(),
    };

    dbg!(&params);
    let paths = enso_build::paths::generated::Paths::new(&params, &PathBuf::from("."));
    std::env::set_var("ENSO_IDE_DIST", &paths.repo_root.dist.path);
    // let versions = Versions::new(Version::parse("2022.1.1-nightly.2022-02-03")?);
    // versions.publish()?;

    let info_for_js = BuildInfo {
        commit:  "badf00d".into(),
        name:    "Enso IDE".into(),
        version: triple.versions.version.clone(),
    };

    ide_ci::io::write(
        &paths.repo_root.app.ide_desktop.join("build.json"),
        serde_json::to_string(&info_for_js)?,
    )?;


    if true {
        // let pm_source = ProjectManagerSource::Local;
        let pm_source = ProjectManagerSource::Bundle(
            paths.repo_root.built_distribution.project_manager_bundle_triple.enso.path.clone(),
        );


        let get_pm_fut = {
            let paths = paths.clone();
            let triple = triple.clone();
            async move { pm_source.get(paths, triple).await }
        }
        .boxed();

        let handle = tokio::task::spawn(get_pm_fut);
        let get_wasm_fut = {
            let paths = paths.clone();
            async move { build_wasm(&paths).await }.boxed()
        };

        let pm_handle = tokio::task::spawn(get_wasm_fut);
        let res = try_join_all([handle, pm_handle], FutureParallelism).await?;
        Result::from_iter(res)?;
    }
    // ProjectManagerSource::Local.get(&paths, &triple).await?;
    // build_wasm(&paths).await?;
    // if (!argv.dev) {
    //     console.log('Minimizing the WASM binary.')
    //                 await gzip(paths.wasm.main, paths.wasm.mainGz)
    //
    //     const limitMb = 4.6
    //     await checkWasmSize(paths.wasm.mainGz, limitMb)
    // }
    // Copy WASM files from temporary directory to Webpack's `dist` directory.
    // ide_ci::io::copy(paths.wasm(), paths.dist_wasm())?;

    // let pm = tokio::task::spawn(get_pm);
    // let wasm = tokio::task::spawn(build_wasm(&paths));
    // let a = try_join_all([pm, wasm], TaskParallelism).await?;

    // // JS PART

    // Lerna.cmd()?.arg("bootstrap").current_dir(&paths.repo_root.app.ide_desktop).run_ok().await?;
    // Npm.args(["run", "install"])?.current_dir(&paths.repo_root.app.ide_desktop).run_ok().await?;



    download_js_assets(&paths).await?;

    Npm.install(&paths.repo_root.app.ide_desktop)?.run_ok().await?;

    Npm.cmd()?
        .args(["--workspace", "enso-studio-icons"])
        .args(["run", "build"])
        .arg(&paths.repo_root.dist.icons)
        .current_dir(&paths.repo_root.app.ide_desktop)
        .run_ok()
        .await?;

    Npm.cmd()?
        .args(["--workspace", "enso-studio-content"])
        .args(["run", "build"])
        .current_dir(&paths.repo_root.app.ide_desktop)
        .run_ok()
        .await?;

    Npm.cmd()?
        .args(["--workspace", "enso"])
        .args(["run", "build"])
        .current_dir(&paths.repo_root.app.ide_desktop)
        .run_ok()
        .await?;

    Npm.cmd()?
        .args(["--workspace", "enso"])
        .args(["run", "dist"]) // , "--", "--win", "dir"
        .current_dir(&paths.repo_root.app.ide_desktop)
        .run_ok()
        .await?;


    println!("{}", paths.temp.display());
    std::mem::forget(paths.temp);

    // Command::new(r"H:\NBO\enso5\dist\client\win-unpacked\Enso.exe").run_ok().await?;

    // Npm.cmd()?.current_dir(&paths.repo_root.app.ide_desktop).args(["run",
    // "dist"]).run_ok().await?;

    Ok(())
}

#[cfg(test)]
mod tests {}
