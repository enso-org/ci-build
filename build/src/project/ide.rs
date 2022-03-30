use crate::paths::generated::RepoRoot;
use crate::prelude::*;
use crate::project::IsTarget;
use futures_util::future::try_join;
use ide_ci::actions::artifacts::upload_directory;
use ide_ci::actions::artifacts::upload_single_file;
use platforms::TARGET_OS;
use std::lazy::SyncLazy;

pub struct Artifact {
    pub unpacked:       PathBuf,
    pub image:          PathBuf,
    pub image_checksum: PathBuf,
}

impl Artifact {
    fn new(version: &Version, dist_dir: &Path) -> Self {
        let unpacked = dist_dir.join(match TARGET_OS {
            OS::Linux => "linux-unpacked",
            OS::MacOS => "mac",
            OS::Windows => "win-unpacked",
            _ => todo!("{TARGET_OS} is not supported"),
        });
        let image = dist_dir.join(match TARGET_OS {
            OS::Linux => format!("enso-linux-{}.AppImage", version),
            OS::MacOS => format!("enso-mac-{}.dmg", version),
            OS::Windows => format!("enso-win-{}.dmg", version),
            _ => todo!("{TARGET_OS} is not supported"),
        });

        Self { image_checksum: image.with_extension("sha256"), image, unpacked }
    }

    pub async fn upload(&self) -> Result {
        upload_directory(&self.unpacked, format!("ide-unpacked-{}", TARGET_OS)).await?;
        upload_single_file(&self.image, format!("ide-{}", TARGET_OS)).await?;
        upload_single_file(&self.image_checksum, format!("ide-{}", TARGET_OS)).await?;
        Ok(())
    }
}

pub struct BuildInput {
    pub repo_root:       RepoRoot,
    pub version:         Version,
    pub project_manager: BoxFuture<'static, Result<crate::project::project_manager::Artifact>>,
    pub gui:             BoxFuture<'static, Result<crate::project::gui::Artifact>>,
}

#[derive(Clone, Debug)]
pub struct Ide;

impl Ide {
    pub fn build(
        &self,
        input: BuildInput,
        output_path: impl AsRef<Path> + Send + Sync + 'static,
    ) -> BoxFuture<'static, Result<Artifact>> {
        let ide_desktop = crate::ide::web::IdeDesktop::new(&input.repo_root.app.ide_desktop);
        async move {
            let (gui, project_manager) = try_join(input.gui, input.project_manager).await?;
            ide_desktop.dist(&gui, &project_manager, &output_path).await?;
            // Ok(output_path.as_ref().to_path_buf());
            Ok(Artifact::new(&input.version, output_path.as_ref()))
        }
        .boxed()
    }
}
