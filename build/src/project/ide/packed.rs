use crate::prelude::*;
use crate::project::ide::unpacked;
// use crate::project::ide::IsIdeArtifact;
// use ide_ci::actions::artifacts::upload_single_file;
// use ide_ci::actions::workflow::is_in_env;
//

pub use unpacked::BuildInput;
use crate::project::{Context, IsTarget};
use crate::source::BuildTargetJob;

pub struct Artifact {
    /// Building a packed IDE image always involves building an unpacked one first.
    pub unpacked:       crate::project::ide::unpacked::Artifact,
    /// File with the compressed client image (like installer or AppImage).
    pub image:          PathBuf,
    /// File with the checksum of the image.
    pub image_checksum: PathBuf,
}

impl Artifact {
    pub fn new(target_os: OS, target_arch: Arch, dist_dir: impl AsRef<Path>
               version: &Version,) -> Self {
        let unpacked = unpacked::Artifact::new(target_os, target_arch, dist_dir);
        let image = dist_dir.as_ref().join(match target_os {
            OS::Linux => format!("enso-linux-{}.AppImage", version),
            OS::MacOS => format!("enso-mac-{}.dmg", version),
            OS::Windows => format!("enso-win-{}.exe", version),
            _ => todo!("{target_os}-{target_arch} combination is not supported"),
        });
        let image_checksum = image.with_extension("sha256");
        Self { image_checksum, image, unpacked }
    }
}

pub struct Ide(pub unpacked::Ide);

impl IsTarget for Ide {
    type BuildInput = BuildInput;
    type Artifact = Artifact;

    fn artifact_name(&self) -> String {
        format!("ide-{}", TARGET_OS)
    }

    fn adapt_artifact(self, path: impl AsRef<Path>) -> BoxFuture<'static, Result<Self::Artifact>> {
        Artifact::new(TARGET_OS, TARGET_ARCH, path).boxed()
    }

    fn build_internal(&self, context: Context, job: BuildTargetJob<Self>) -> BoxFuture<'static, Result<Self::Artifact>> {
        todo!()
    }
}
// impl Artifact {
//     pub fn new(target_os: OS, target_arch: Arch, dist_dir: impl AsRef<Path>
//                version: &Version,) -> Self {
//         let unpacked = unpacked::Artifact::new(target_os, target_arch, dist_dir);
//         let image = dist_dir.as_ref().join(match target_os {
//             OS::Linux => format!("enso-linux-{}.AppImage", version),
//             OS::MacOS => format!("enso-mac-{}.dmg", version),
//             OS::Windows => format!("enso-win-{}.exe", version),
//             _ => todo!("{target_os}-{target_arch} combination is not supported"),
//         });
//         let image_checksum = image.with_extension("sha256");
//         Self { image_checksum, image, unpacked }
//     }
// }
//
// impl IsIdeArtifact for Artifact {
//     fn unpacked_executable(&self) -> &Path {
//         self.unpacked.unpacked_executable()
//     }
//
//     fn upload_artifacts_job(&self) -> BoxFuture<Result> {
//         async move {
//             let artifact_name = format!("ide-{}", TARGET_OS);
//             upload_single_file(&self.image_checksum, &artifact_name).await?;
//             upload_single_file(&self.image, &artifact_name).await?;
//             self.unpacked.upload_as_ci_artifact().await
//         }
//         .boxed()
//     }
// }
