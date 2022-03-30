use crate::prelude::*;
use futures_util::future::try_join;
use futures_util::future::try_join3;
use tempfile::TempDir;

use crate::paths::generated;
use crate::project::gui::BuildInfo;
use crate::project::wasm::Artifacts;
use ide_ci::io::download_all;
use ide_ci::program::EMPTY_ARGS;
use ide_ci::programs::node::NpmCommand;
use ide_ci::programs::Npm;

lazy_static! {
    /// Path to the file with build information that is consumed by the JS part of the IDE.
    ///
    /// The file must follow the schema of type [`BuildInfo`].
    pub static ref BUILD_INFO: PathBuf = PathBuf::from("build.json");
}

pub const IDE_ASSETS_URL: &str =
    "https://github.com/enso-org/ide-assets/archive/refs/heads/main.zip";

pub const ARCHIVED_ASSET_FILE: &str = "ide-assets-main/content/assets/";


pub mod env {
    use super::*;

    pub struct IdeDistPath;
    impl EnvironmentVariable for IdeDistPath {
        const NAME: &'static str = "ENSO_BUILD_IDE";
        type Value = PathBuf;
    }

    pub struct ProjectManager;
    impl EnvironmentVariable for ProjectManager {
        const NAME: &'static str = "ENSO_BUILD_PROJECT_MANAGER";
        type Value = PathBuf;
    }

    pub struct GuiDistPath;
    impl EnvironmentVariable for GuiDistPath {
        const NAME: &'static str = "ENSO_BUILD_GUI";
        type Value = PathBuf;
    }

    pub struct IconsPath;
    impl EnvironmentVariable for IconsPath {
        const NAME: &'static str = "ENSO_BUILD_ICONS";
        type Value = PathBuf;
    }

    pub struct WasmPath;
    impl EnvironmentVariable for WasmPath {
        const NAME: &'static str = "ENSO_BUILD_GUI_WASM";
        type Value = PathBuf;
    }

    pub struct JsGluePath;
    impl EnvironmentVariable for JsGluePath {
        const NAME: &'static str = "ENSO_BUILD_GUI_JS_GLUE";
        type Value = PathBuf;
    }

    pub struct AssetsPath;
    impl EnvironmentVariable for AssetsPath {
        const NAME: &'static str = "ENSO_BUILD_GUI_ASSETS";
        type Value = PathBuf;
    }
}

#[derive(Clone, Debug)]
pub struct IconsArtifacts(pub PathBuf);


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
    /// The Electron client.
    Enso,
}

impl AsRef<OsStr> for Workspaces {
    fn as_ref(&self) -> &OsStr {
        match self {
            Workspaces::Icons => OsStr::new("enso-studio-icons"),
            Workspaces::Content => OsStr::new("enso-studio-content"),
            Workspaces::Enso => OsStr::new("enso"),
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

    pub async fn build_icons(&self, output_path: impl AsRef<Path>) -> Result<IconsArtifacts> {
        env::IconsPath.set_path(&output_path);
        self.npm()?.workspace(Workspaces::Icons).run("build", EMPTY_ARGS).run_ok().await?;
        Ok(IconsArtifacts(output_path.as_ref().into()))
    }

    pub async fn build(
        &self,
        wasm: impl Future<Output = Result<Artifacts>>,
        build_info: &BuildInfo,
        output_path: impl AsRef<Path>,
    ) -> Result {
        let installation = self.install();

        let assets = TempDir::new()?;
        let assets_download = download_js_assets(&assets);

        let (wasm, _, _) = try_join3(wasm, installation, assets_download).await?;

        self.write_build_info(&build_info)?;

        // TODO: this should be set only on a child processes
        env::GuiDistPath.set_path(&output_path);
        env::WasmPath.set_path(&wasm.wasm());
        env::JsGluePath.set_path(&wasm.js_glue());
        env::AssetsPath.set_path(&assets);
        self.npm()?.workspace(Workspaces::Content).run("build", EMPTY_ARGS).run_ok().await?;
        Ok(())
    }

    pub async fn watch(
        &self,
        wasm: impl Future<Output = Result<Artifacts>>,
        build_info: &BuildInfo,
    ) -> Result {
        // TODO deduplicate with build
        self.install().await?;

        let wasm = wasm.await?;
        let assets = TempDir::new()?;
        download_js_assets(&assets).await?;

        self.write_build_info(&build_info)?;

        // TODO: this should be set only on a child processes
        let output_path = TempDir::new()?;
        env::GuiDistPath.set_path(&output_path);
        env::WasmPath.set_path(&wasm.wasm());
        env::JsGluePath.set_path(&wasm.js_glue());
        env::AssetsPath.set_path(&assets);
        self.npm()?.workspace(Workspaces::Content).run("watch", EMPTY_ARGS).run_ok().await?;
        Ok(())
    }

    pub async fn dist(
        &self,
        gui: &crate::project::gui::Artifact,
        project_manager: &crate::project::project_manager::Artifact,
        output_path: impl AsRef<Path>,
    ) -> Result {
        self.install().await?;
        env::GuiDistPath.set_path(&gui);
        env::ProjectManager.set_path(&project_manager);
        env::IdeDistPath.set_path(&output_path);
        let content_build =
            self.npm()?.workspace(Workspaces::Enso).run("build", EMPTY_ARGS).run_ok();

        // &input.repo_root.dist.icons
        let icons_dist = TempDir::new()?;
        let icons_build = self.build_icons(&icons_dist);
        try_join(icons_build, content_build).await?;
        env::IconsPath.set_path(&icons_dist);
        self.npm()?.workspace(Workspaces::Enso).run("dist", EMPTY_ARGS).run_ok().await?;
        Ok(())
    }
}

impl From<&generated::RepoRoot> for IdeDesktop {
    fn from(value: &generated::RepoRoot) -> Self {
        Self { package_dir: value.app.ide_desktop.to_path_buf() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn download_test() -> Result {
        let temp = TempDir::new()?;
        download_js_assets(temp.path()).await?;
        Ok(())
    }
}
