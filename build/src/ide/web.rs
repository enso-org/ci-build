use crate::prelude::*;

use crate::paths::generated;
use crate::project::gui::BuildInfo;
use crate::project::wasm;
use crate::project::wasm::Artifact;
use crate::project::ProcessWrapper;

use anyhow::Context;
use futures_util::future::try_join;
use futures_util::future::try_join3;
use ide_ci::io::download_all;
use ide_ci::program::command;
use ide_ci::program::EMPTY_ARGS;
use ide_ci::programs::node::NpmCommand;
use ide_ci::programs::Npm;
use std::process::Stdio;
use tempfile::TempDir;
use tokio::process::Child;
use tracing::Span;

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
    use ide_ci::define_env_var;

    define_env_var!(ENSO_BUILD_IDE, PathBuf);
    define_env_var!(ENSO_BUILD_PROJECT_MANAGER, PathBuf);
    define_env_var!(ENSO_BUILD_GUI, PathBuf);
    define_env_var!(ENSO_BUILD_ICONS, PathBuf);
    define_env_var!(ENSO_BUILD_GUI_WASM, PathBuf);
    define_env_var!(ENSO_BUILD_GUI_JS_GLUE, PathBuf);
    define_env_var!(ENSO_BUILD_GUI_ASSETS, PathBuf);
    define_env_var!(ENSO_BUILD_IDE_BUNDLED_ENGINE_VERSION, Version);
    define_env_var!(ENSO_BUILD_PROJECT_MANAGER_IN_BUNDLE_PATH, PathBuf);
}

#[derive(Clone, Debug)]
pub struct IconsArtifacts(pub PathBuf);

impl command::FallibleManipulator for IconsArtifacts {
    fn try_applying<C: IsCommandWrapper + ?Sized>(&self, command: &mut C) -> Result {
        command.set_env(env::ENSO_BUILD_ICONS, &self.0)?;
        Ok(())
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

/// Things that are common to `watch` and `build`.
#[derive(Debug)]
pub struct ContentEnvironment<Assets, Output> {
    asset_dir:   Assets,
    wasm:        wasm::Artifact,
    output_path: Output,
}

impl<Output: AsRef<Path>> ContentEnvironment<TempDir, Output> {
    pub async fn new(
        ide: &IdeDesktop,
        wasm: impl Future<Output = Result<Artifact>>,
        build_info: &BuildInfo,
        output_path: Output,
    ) -> Result<Self> {
        let installation = ide.install();
        let asset_dir = TempDir::new()?;
        let assets_download = download_js_assets(&asset_dir);
        let (wasm, _, _) = try_join3(wasm, installation, assets_download).await?;
        ide.write_build_info(&build_info)?;
        Ok(ContentEnvironment { asset_dir, wasm, output_path })
    }
}

impl<Assets: AsRef<Path>, Output: AsRef<Path>> command::FallibleManipulator
    for ContentEnvironment<Assets, Output>
{
    fn try_applying<C: IsCommandWrapper + ?Sized>(&self, command: &mut C) -> Result {
        command
            .set_env(env::ENSO_BUILD_GUI, self.output_path.as_ref())?
            .set_env(env::ENSO_BUILD_GUI_WASM, &self.wasm.wasm())?
            .set_env(env::ENSO_BUILD_GUI_JS_GLUE, &self.wasm.js_glue())?
            .set_env(env::ENSO_BUILD_GUI_ASSETS, self.asset_dir.as_ref())?;
        Ok(())
    }
}

impl<Assets, Output> Drop for ContentEnvironment<Assets, Output> {
    fn drop(&mut self) {
        info!("Dropping content environment.")
    }
}

pub fn target_flag(os: OS) -> Result<&'static str> {
    match os {
        OS::Windows => Ok("--win"),
        OS::Linux => Ok("--linux"),
        OS::MacOS => Ok("--mac"),
        _ => bail!("Not supported target for Electron client: {os}."),
    }
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
        command.arg("--color").arg("always");
        command.arg("--yes");
        command.current_dir(&self.package_dir);
        command.stdin(Stdio::null()); // nothing in that process subtree should require input
        Ok(command)
    }

    pub fn write_build_info(&self, info: &BuildInfo) -> Result {
        let path = self.package_dir.join(&*BUILD_INFO);
        path.write_as_json(info)
    }

    pub async fn install(&self) -> Result {
        self.npm()?.install().arg("--workspaces").arg("--verbose").run_ok().await?;
        Ok(())
    }

    pub async fn build_icons(&self, output_path: impl AsRef<Path>) -> Result<IconsArtifacts> {
        self.npm()?
            .workspace(Workspaces::Icons)
            .set_env(env::ENSO_BUILD_ICONS, output_path.as_ref())?
            .run("build", EMPTY_ARGS)
            .run_ok()
            .await?;
        Ok(IconsArtifacts(output_path.as_ref().into()))
    }

    #[tracing::instrument(name="Building IDE Content.", skip_all, fields(
        dest = %output_path.as_ref().display(),
        build_info,
        err))]
    pub async fn build_content(
        &self,
        wasm: impl Future<Output = Result<Artifact>>,
        build_info: &BuildInfo,
        output_path: impl AsRef<Path>,
    ) -> Result {
        let env = ContentEnvironment::new(self, wasm, build_info, output_path).await?;
        //env.apply();
        self.npm()?
            .try_applying(&env)?
            .workspace(Workspaces::Content)
            .run("build", EMPTY_ARGS)
            .run_ok()
            .await?;

        debug!(assets=?env.asset_dir, "Still kept");
        drop(env); // does this extend the lifetime?
        Ok(())
    }


    #[tracing::instrument(name="Setting up GUI Content watcher.", 
        fields(wasm = tracing::field::Empty),
        err)]
    pub async fn watch_content(
        &self,
        wasm: impl Future<Output = Result<Artifact>>,
        build_info: &BuildInfo,
        shell: bool,
    ) -> Result<Watcher> {
        // When watching we expect our artifacts to be served through server, not appear in any
        // specific location on the disk.
        let output_path = TempDir::new()?;
        // let span = tracing::
        // let wasm = wasm.inspect()
        let watch_environment =
            ContentEnvironment::new(self, wasm, build_info, output_path).await?;
        Span::current().record("wasm", &watch_environment.wasm.as_str());
        let child_process = if shell {
            ide_ci::os::default_shell()
                .cmd()?
                .current_dir(&self.package_dir)
                .try_applying(&watch_environment)?
                .stdin(Stdio::inherit())
                .spawn()?
        } else {
            self.npm()?
                .try_applying(&watch_environment)?
                .workspace(Workspaces::Content)
                .run("watch", EMPTY_ARGS)
                .spawn_intercepting()?
        };
        Ok(Watcher { child_process, watch_environment })
    }

    #[tracing::instrument(name="Preparing distribution of the IDE.", skip_all, fields(
        dest = %output_path.as_ref().display(),
        ?gui,
        ?project_manager,
        ?target_os,
        err))]
    pub async fn dist(
        &self,
        gui: &crate::project::gui::Artifact,
        project_manager: &crate::project::backend::Artifact,
        output_path: impl AsRef<Path>,
        target_os: OS,
    ) -> Result {
        self.npm()?.install().run_ok().await?;

        let engine_version_to_use = project_manager.engine_versions.iter().max();
        if engine_version_to_use.is_none() {
            warn!("Bundled Project Manager does not contain any Engine.");
        }

        let pm_in_bundle = project_manager
            .path
            .bin
            .project_managerexe
            .strip_prefix(&project_manager.path)
            .context("Failed to generate in-bundle path to Project Manager executable")?;

        let content_build = self
            .npm()?
            .set_env(env::ENSO_BUILD_GUI, gui.as_ref())?
            .set_env(env::ENSO_BUILD_PROJECT_MANAGER, project_manager.as_ref())?
            .set_env(env::ENSO_BUILD_IDE, output_path.as_ref())?
            .set_env_opt(env::ENSO_BUILD_IDE_BUNDLED_ENGINE_VERSION, engine_version_to_use)?
            .set_env(env::ENSO_BUILD_PROJECT_MANAGER_IN_BUNDLE_PATH, pm_in_bundle)?
            .workspace(Workspaces::Enso)
            .run("build", EMPTY_ARGS)
            .run_ok();

        // &input.repo_root.dist.icons
        let icons_dist = TempDir::new()?;
        let icons_build = self.build_icons(&icons_dist);
        let (icons, _content) = try_join(icons_build, content_build).await?;


        self.npm()?
            .try_applying(&icons)?
            // .env("DEBUG", "electron-builder")
            .set_env(env::ENSO_BUILD_GUI, gui.as_ref())?
            .set_env(env::ENSO_BUILD_IDE, output_path.as_ref())?
            .set_env(env::ENSO_BUILD_PROJECT_MANAGER, project_manager.as_ref())?
            .workspace(Workspaces::Enso)
            // .args(["--loglevel", "verbose"])
            .run("dist", EMPTY_ARGS)
            .arg("--")
            .arg(target_flag(target_os)?)
            .run_ok()
            .await?;

        Ok(())
    }
}

impl From<&generated::RepoRoot> for IdeDesktop {
    fn from(value: &generated::RepoRoot) -> Self {
        Self { package_dir: value.app.ide_desktop.to_path_buf() }
    }
}

#[derive(Debug)]
pub struct Watcher {
    pub watch_environment: ContentEnvironment<TempDir, TempDir>,
    pub child_process:     Child,
}

impl ProcessWrapper for Watcher {
    fn inner(&mut self) -> &mut Child {
        &mut self.child_process
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
