use crate::prelude::*;
use ide_ci::actions::workflow::is_in_env;
use tempfile::TempDir;

use crate::ide::wasm::WasmArtifacts;
use crate::ide::BuildInfo;
use crate::paths::TargetTriple;
use ide_ci::io::download_all;
use ide_ci::program::EMPTY_ARGS;
use ide_ci::programs::node::NpmCommand;
use ide_ci::programs::Npm;

/// Path to the file with build information that is consumed by the JS part of the IDE.
///
/// The file must follow the schema of type [`BuildInfo`].
lazy_static! {
    pub static ref BUILD_INFO: PathBuf = PathBuf::from("build.json");
}

pub const IDE_ASSETS_URL: &str =
    "https://github.com/enso-org/ide-assets/archive/refs/heads/main.zip";

pub const ARCHIVED_ASSET_FILE: &str = "ide-assets-main/content/assets/";


pub mod env {
    use super::*;

    pub struct OutputPath;
    impl EnvironmentVariable for OutputPath {
        const NAME: &'static str = "ENSO_IDE_DIST";
        type Value = PathBuf;
    }

    pub struct IconsPath;
    impl EnvironmentVariable for IconsPath {
        const NAME: &'static str = "ENSO_ICONS";
        type Value = PathBuf;
    }

    pub struct WasmPath;
    impl EnvironmentVariable for WasmPath {
        const NAME: &'static str = "ENSO_GUI_WASM";
        type Value = PathBuf;
    }

    pub struct JsGluePath;
    impl EnvironmentVariable for JsGluePath {
        const NAME: &'static str = "ENSO_GUI_JS_GLUE";
        type Value = PathBuf;
    }

    pub struct AssetsPath;
    impl EnvironmentVariable for AssetsPath {
        const NAME: &'static str = "ENSO_GUI_ASSETS";
        type Value = PathBuf;
    }
}


/// Fill the directory under `output_path` with the assets.
pub async fn download_js_assets(output_path: impl AsRef<Path>) -> Result {
    let output = output_path.as_ref();
    let archived_asset_prefix = PathBuf::from(ARCHIVED_ASSET_FILE);
    let archive = download_all(IDE_ASSETS_URL).await?;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(archive))?;
    ide_ci::archive::zip::extract_subtree(&mut archive, &archived_asset_prefix, &output)?;
    Ok(())
}
//
// pub trait Inputs {
//     fn wasm(&self) -> PathBuf;
//     fn js_glue(&self) -> PathBuf;
//     fn output_path(&self) -> PathBuf;
//     fn build_info(&self) -> BuildInfo;
// }

pub enum Workspaces {
    Icons,
    Content,
}

impl AsRef<OsStr> for Workspaces {
    fn as_ref(&self) -> &OsStr {
        match self {
            Workspaces::Icons => OsStr::new("enso-studio-icons"),
            Workspaces::Content => OsStr::new("enso-studio-content"),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Command {
    Build,
    Watch,
}

#[derive(Clone, Debug)]
pub struct IdeDesktop {
    pub package_dir: PathBuf,
}

impl IdeDesktop {
    pub fn new(package_dir: impl Into<PathBuf>) -> Self {
        Self { package_dir: package_dir.into() }
    }

    pub fn npm(&self) -> Result<NpmCommand> {
        let mut command = Npm.cmd()?;
        command.current_dir(&self.package_dir);
        Ok(command)
    }

    pub fn write_build_info(&self, info: &BuildInfo) -> Result {
        let path = self.package_dir.join(&*BUILD_INFO);
        ide_ci::fs::write(&path, serde_json::to_string(&info)?)
    }

    pub async fn install(&self) -> Result {
        self.npm()?.install().run_ok().await
    }

    pub async fn build(
        &self,
        wasm: &WasmArtifacts,
        build_info: &BuildInfo,
        output_path: impl AsRef<Path>,
    ) -> Result {
        self.install();

        let assets = TempDir::new()?;
        download_js_assets(&assets).await?;

        self.write_build_info(&build_info)?;

        // TODO: this should be set only on a child processes
        env::OutputPath.set_path(&output_path);
        env::WasmPath.set_path(&wasm.wasm);
        env::JsGluePath.set_path(&wasm.js_glue);
        env::AssetsPath.set_path(&assets);

        self.npm()?.workspace(Workspaces::Content).run("build", EMPTY_ARGS).run_ok().await?;

        if is_in_env() {
            ide_ci::actions::artifacts::upload_directory(output_path.as_ref(), "gui_content")
                .await?;
        }

        Ok(())
    }

    pub async fn watch(&self, wasm: &WasmArtifacts, build_info: &BuildInfo) -> Result {
        self.install().await?;

        let assets = TempDir::new()?;
        download_js_assets(&assets).await?;

        self.write_build_info(&build_info)?;

        // TODO: this should be set only on a child processes
        let output_path = TempDir::new()?;
        env::OutputPath.set_path(&output_path);
        env::WasmPath.set_path(&wasm.wasm);
        env::JsGluePath.set_path(&wasm.js_glue);
        env::AssetsPath.set_path(&assets);
        self.npm()?.workspace(Workspaces::Content).run("watch", EMPTY_ARGS).run_ok().await?;
        Ok(())
    }
}
