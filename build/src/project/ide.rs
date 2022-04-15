use crate::prelude::*;

use crate::paths::generated::RepoRoot;

use futures_util::future::try_join;
use ide_ci::actions::artifacts::upload_compressed_directory;
use ide_ci::actions::artifacts::upload_single_file;
use ide_ci::actions::workflow::is_in_env;
use platforms::TARGET_OS;

pub struct Artifact {
    pub unpacked:       PathBuf,
    // pub packed:         PathBuf,
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
            OS::Windows => format!("enso-win-{}.exe", version),
            _ => todo!("{TARGET_OS} is not supported"),
        });

        Self { image_checksum: image.with_extension("sha256"), image, unpacked }
    }

    pub async fn upload(&self) -> Result {
        if is_in_env() {
            upload_compressed_directory(&self.unpacked, format!("ide-unpacked-{}", TARGET_OS))
                .await?;
            upload_single_file(&self.image, format!("ide-{}", TARGET_OS)).await?;
            upload_single_file(&self.image_checksum, format!("ide-{}", TARGET_OS)).await?;
        } else {
            info!("Not in the CI environment, will not upload the artifacts.")
        }
        Ok(())
    }
}
//
// impl IsArtifact for Artifact {
//     fn from_existing(path: impl AsRef<Path>) -> BoxFuture<'static, Result<Self>> {
//         todo!()
//     }
// }

#[derive(derivative::Derivative)]
#[derivative(Debug)]
pub struct BuildInput {
    pub repo_root:       RepoRoot,
    pub version:         Version,
    #[derivative(Debug = "ignore")]
    pub project_manager: BoxFuture<'static, Result<crate::project::project_manager::Artifact>>,
    #[derivative(Debug = "ignore")]
    pub gui:             BoxFuture<'static, Result<crate::project::gui::Artifact>>,
}

pub enum OutputPath {
    /// The job must place the artifact under given path.
    Required(PathBuf),
    /// THe job may place the artifact anywhere, though it should use the suggested path if it has
    /// no "better idea" (like reusing existing cache).
    Suggested(PathBuf),
    /// The job is responsible for finding a place for artifacts.
    Whatever,
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
            Ok(Artifact::new(&input.version, output_path.as_ref()))
        }
        .boxed()
    }
}

// impl IsTarget for Ide {
//     type BuildInput = BuildInput;
//     type Output = Artifact;
//
//     fn artifact_name(&self) -> &str {
//         // Version is not part of the name intentionally. We want to refer to PM bundles as
//         // artifacts without knowing their version.
//         static NAME: SyncLazy<String> = SyncLazy::new(|| format!("gui-{}", TARGET_OS));
//         &*NAME
//     }
//
//     fn build(
//         &self,
//         input: Self::BuildInput,
//         output_path: impl AsRef<Path> + Send + Sync + 'static,
//     ) -> BoxFuture<'static, Result<Self::Output>> {
//         let ide_desktop = crate::ide::web::IdeDesktop::new(&input.repo_root.app.ide_desktop);
//         async move {
//             let (gui, project_manager) = try_join(input.gui, input.project_manager).await?;
//             ide_desktop.dist(&gui, &project_manager, &output_path).await?;
//             Ok(Artifact::new(&input.version, output_path.as_ref()))
//         }
//         .boxed()
//     }
// }
