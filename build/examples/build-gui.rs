#![feature(exit_status_error)]

use anyhow::Context;
use enso_build::paths::GuiPaths;
use enso_build::paths::TargetTriple;
use enso_build::prelude::*;
use ide_ci::io::create_dir_if_missing;
use ide_ci::io::download_all;
use ide_ci::io::download_and_extract;
use ide_ci::programs::Npm;
use regex::Regex;
// use regex::Regex;
use enso_build::paths::generated::Paths;
use enso_build::version::Versions;
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
            extract_file(&mut file, output)?;
        }
    }
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


#[tokio::main]
async fn main() -> Result {
    let root_path = PathBuf::from("H:/NBO/enso5");
    let temp = tempfile::tempdir()?;

    //let temp = temp.path();
    let path = PathBuf::from(r"C:\temp\qwert");
    let params = enso_build::paths::generated::Parameters {
        repo_root: root_path.clone(),
        temp:      temp.path().to_owned(),
    };
    dbg!(&params);
    let paths = enso_build::paths::generated::Paths::new(&params, &PathBuf::from("."));
    let versions = Versions::new(Version::parse("2022.1.1-nightly.2022-02-03")?);
    let target = TargetTriple::new(versions);
    let env = std::env::vars().filter(|(name, _val)| !name.starts_with("CARGO"));

    let target_crate = "app/gui";


    let is_dev = false;

    dbg!(&paths);
    init(&paths).await?;


    let mut cmd = tokio::process::Command::new("wasm-pack");
    cmd.env_remove("RUSTUP_TOOLCHAIN");
    cmd.args([
        "build",
        "--target",
        "web",
        "--out-dir",
        paths.temp.enso_wasm.as_str(), // &paths.wasm().as_os_str().to_str().unwrap(),
        "--out-name",
        "ide",
        target_crate,
    ])
    .current_dir(&paths.repo_root)
    .spawn()?
    .wait()
    .await?
    .exit_ok()?;

    patch_file(&paths.temp.enso_wasm.ide_js, js_workaround_patcher)?;
    std::fs::rename(&paths.temp.enso_wasm.ide_bg_wasm, &paths.temp.enso_wasm.ide_wasm)?;
    ide_ci::io::copy(&paths.temp.enso_wasm, &paths.repo_root.dist.wasm)?;

    // dbg!(paths);

    // if (!argv.dev) {
    //     console.log('Minimizing the WASM binary.')
    //                 await gzip(paths.wasm.main, paths.wasm.mainGz)
    //
    //     const limitMb = 4.6
    //     await checkWasmSize(paths.wasm.mainGz, limitMb)
    // }
    // Copy WASM files from temporary directory to Webpack's `dist` directory.
    // ide_ci::io::copy(paths.wasm(), paths.dist_wasm())?;



    // // JS PART
    Npm.args(["run", "install"])?.current_dir(&paths.repo_root.app.ide_desktop).run_ok().await?;
    download_js_assets(&paths).await?;
    enso_build::project_manager::ensure_present(&paths.repo_root.dist.project_manager, &target)
        .await?;

    Npm.cmd()?.current_dir(&paths.repo_root.app.ide_desktop).args(["run", "dist"]).run_ok().await?;

    println!("{}", paths.temp.display());
    std::mem::forget(paths.temp);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
