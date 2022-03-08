#![feature(exit_status_error)]

use enso_build::prelude::*;

use anyhow::Context;
use enso_build::engine::BuildConfiguration;
use enso_build::engine::BuildOperation;
use enso_build::paths::generated::Paths;
use enso_build::paths::GuiPaths;
use enso_build::paths::TargetTriple;
use enso_build::setup_octocrab;
use enso_build::version::Versions;
use ide_ci::env::Variable;
use ide_ci::future::try_join_all;
use ide_ci::future::AsyncPolicy::FutureParallelism;
use ide_ci::goodie::GoodieDatabase;
use ide_ci::io::create_dir_if_missing;
use ide_ci::io::download_all;
use ide_ci::programs::npx::Npx;
use ide_ci::programs::Lerna::Lerna;
use ide_ci::programs::Node;
use ide_ci::programs::Npm;
use regex::Regex;
use std::time::Duration;
use tempfile::TempDir;
use zip::read::ZipFile;

/// Workaround fix by wdanilo, see: https://github.com/rustwasm/wasm-pack/issues/790
pub fn js_workaround_patcher(code: impl Into<String>) -> Result<String> {
    let replacements = [
        (r"(?s)if \(typeof input === 'string'.*return wasm;", "return imports"),
        (r"(?s)if \(typeof input === 'undefined'.*const imports = \{\};", "const imports = {};"),
        (r"(?s)export default init;", "export default init"),
    ];

    let mut ret = code.into();
    for (regex, replacement) in replacements {
        let regex = Regex::new(regex).unwrap();
        ret = regex.replace_all(&ret, replacement).to_string();
    }

    ret.push_str("\nexport function after_load(w,m) { wasm = w; init.__wbindgen_wasm_module = m;}");
    Ok(ret)
}

pub fn patch_file(
    path: impl AsRef<Path>,
    patcher: impl FnOnce(String) -> Result<String>,
) -> Result {
    println!("Patching {}.", path.as_ref().display());
    let original_content = std::fs::read_to_string(&path)?;
    let patched_content = patcher(original_content)?;
    std::fs::write(path, patched_content)?;
    Ok(())
}

pub fn extract_file(file: &mut ZipFile, output: impl AsRef<Path>) -> Result {
    println!("Extracting {}", output.as_ref().display());
    if file.is_dir() {
        create_dir_if_missing(&output)?;
    } else {
        let mut output_file = ide_ci::io::create(&output)?;
        std::io::copy(file, &mut output_file)?;
    }
    // Get and Set permissions
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Some(mode) = file.unix_mode() {
            std::fs::set_permissions(&output, std::fs::Permissions::from_mode(mode))?;
        }
    }
    Ok(())
}


async fn download_js_assets(paths: &Paths) -> Result {
    // let bar = indicatif::ProgressBar::new_spinner();

    let output = paths.repo_root.app.ide_desktop.lib.content.join("assets");
    const ARCHIVED_ASSET_FILE: &str = "ide-assets-main/content/assets/";
    let archived_asset_prefix = PathBuf::from(ARCHIVED_ASSET_FILE);
    let ide_assets_url = "https://github.com/enso-org/ide-assets/archive/refs/heads/main.zip";
    let archive = download_all(ide_assets_url).await?;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(archive))?;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        let path_in_archive = file
            .enclosed_name()
            .context(format!("Illegal path in the archive: {}", file.name()))?;
        if let Ok(relative_path) = path_in_archive.strip_prefix(&archived_asset_prefix) {
            let output = output.join(relative_path);
            // let msg = format!("Extracting {}", output.display());
            // bar.set_message(msg);
            // std::thread::sleep(Duration::from_secs(1));
            extract_file(&mut file, output)?;
        }
    }
    // bar.finish_with_message("Done! :)");
    Ok(())
}

async fn init(paths: &Paths) -> Result {
    let init_token = &paths.repo_root.dist.build_init;
    if !init_token.exists() {
        println!("Initialization");
        println!("Installing build script dependencies.");
        Npm.cmd()?.current_dir(&paths.repo_root.build).arg("install").run_ok().await?;
        ide_ci::io::create_dir_if_missing(&paths.repo_root.dist)?;
        std::fs::write(init_token, "")?;
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

#[derive(Clone, Debug)]
pub enum ProjectManagerSource {
    Local,
    Bundle(PathBuf),
    Release(Version),
}

impl ProjectManagerSource {
    pub async fn get(&self, paths: Paths, triple: TargetTriple) -> Result {
        let repo_root = &paths.repo_root;
        let target_path = &paths.repo_root.dist.project_manager;
        match self {
            ProjectManagerSource::Local => {
                let context = enso_build::engine::context::RunContext {
                    operation: enso_build::engine::Operation::Build(BuildOperation {}),
                    goodies:   GoodieDatabase::new()?,
                    config:    BuildConfiguration {
                        clean_repo: false,
                        build_bundles: true,
                        ..enso_build::engine::NIGHTLY
                    },
                    octocrab:  setup_octocrab()?,
                    paths:     enso_build::paths::Paths::new_version(
                        &repo_root.path,
                        triple.versions.version.clone(),
                    )?,
                };
                dbg!(context.execute().await?);
                ide_ci::io::reset_dir(&target_path)?;
                ide_ci::io::copy_to(
                    &repo_root.built_distribution.project_manager_bundle_triple.enso,
                    &target_path,
                )?;
            }
            ProjectManagerSource::Bundle(path) => {
                ide_ci::io::reset_dir(&target_path)?;
                ide_ci::io::copy_to(
                    &repo_root.built_distribution.project_manager_bundle_triple.enso,
                    &target_path,
                )?;
            }
            ProjectManagerSource::Release(version) => {
                enso_build::project_manager::ensure_present(&target_path, &triple).await?;
            }
        };
        Ok(())
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

    patch_file(&wasm_dir.wasm_glue, js_workaround_patcher)?;
    std::fs::rename(&wasm_dir.wasm_main_raw, &wasm_dir.wasm_main)?;
    Ok(())
}

pub struct BuildInfo {
    pub commit:  String,
    pub version: Version,
    pub name:    String,
}


#[tokio::main]
async fn main() -> Result {
    let root_path = PathBuf::from("H:/NBO/enso5");
    let temp = tempfile::tempdir()?;

    let version = enso_build::version::suggest_new_version();
    let versions = Versions::new(version);
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
    // let versions = Versions::new(Version::parse("2022.1.1-nightly.2022-02-03")?);
    // versions.publish()?;

    if false {
        // let pm_source = ProjectManagerSource::Local;
        let pm_source = ProjectManagerSource::Bundle(
            r"H:\NBO\enso5\built-distribution\project-manager-bundle-2022.1.1-windows-amd64".into(),
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
        let _res = try_join_all([handle, pm_handle], FutureParallelism).await?;
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


    std::env::set_var("ENSO_IDE_DIST", &paths.repo_root.dist.path);

    download_js_assets(&paths).await?;

    Npm.install(&paths.repo_root.app.ide_desktop)?.run_ok().await?;
    Npm.cmd()?
        .args(["--workspace", "enso-studio-icons"])
        .args(["run", "build"])
        .arg(&paths.repo_root.dist.path)
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
        .args(["run", "dist", "--", "--win", "dir"])
        .current_dir(&paths.repo_root.app.ide_desktop)
        .run_ok()
        .await?;


    Command::new(r"H:\NBO\enso5\dist\client\win-unpacked\Enso.exe").run_ok().await;

    // Npm.cmd()?.current_dir(&paths.repo_root.app.ide_desktop).args(["run",
    // "dist"]).run_ok().await?;

    println!("{}", paths.temp.display());
    std::mem::forget(paths.temp);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use platforms::TARGET_OS;
    use serde_json::json;
    //
    // #[test]
    // fn interpolate_hbs() -> Result {
    //     let mut handlebars = handlebars::Handlebars::new();
    //     let data = json! ({
    //         "env": {
    //             "ENSO_VERSION": "3333.333.33"
    //         }
    //     });
    //
    //
    //     for path in glob::glob(r"H:\NBO\enso5\app/**/*.hbs")? {
    //         let path = path?;
    //         if path.as_str().contains("node_modules") {
    //             continue;
    //         };
    //
    //         println!("{}", path.display());
    //         dbg!(handlebars.render_template(&std::fs::read_to_string(path)?, &data));
    //     }
    //     Ok(())
    // }

    #[test]
    fn patcher_test1() -> Result {
        let before = r#"}

async function init(input) {
    if (typeof input === 'undefined') {
        input = new URL('ide_bg.wasm', import.meta.url);
    }
    const imports = {};
    imports.wbg = {};"#;

        let expected = r#"
        }

async function init(input) {
    const imports = {};
    imports.wbg = {};"#;
        let after = js_workaround_patcher(before)?;
        std::fs::write(r"C:\temp\wasm4.js", &after)?;
        assert_eq!(after, expected);
        Ok(())
    }

    #[test]
    fn patcher_test() -> Result {
        let before = std::fs::read_to_string(r"C:\temp\wasm.js.bak")?;
        let expected = std::fs::read_to_string(r"C:\temp\wasm.js")?;

        let after = js_workaround_patcher(before)?;
        std::fs::write(r"C:\temp\wasm3.js", &after)?;
        assert_eq!(after, expected);
        Ok(())
    }

    #[test]
    fn aaaa() -> Result {
        log::warn!("Hello, log!");
        // let a = r#"{"os":"windows","arch":"x86_64","versions":{"version":"2022.1.1-nightly.
        // 2022-02-03","release_mode":true}}"#; dbg!(serde_json::from_str::
        // <TargetTriple>(a));

        // let dist_path = PathBuf::from(r"H:\NBO\enso5\dist");
        // let build_info_file = dist_path.join("installed-enso-version");
        // let content = std::fs::read_to_string(&build_info_file)?;
        //
        // let file = std::fs::File::open(&build_info_file)?;
        // let old_info = dbg!(build_info_file.read_to_json::<TargetTriple>());
        // dbg!(serde_json::from_str::<TargetTriple>(&content));
        // dbg!(serde_json::from_reader::<_, TargetTriple>(file));
        // let os = TARGET_OS;
        // assert_eq!(serde_json::to_string(&os)?, "\"windows\"");
        // assert_eq!(serde_json::from_str::<platforms::target::OS>("\"windows\"")?, TARGET_OS);
        Ok(())
    }
}
