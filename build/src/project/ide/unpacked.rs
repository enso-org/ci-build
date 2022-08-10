use crate::prelude::*;

use crate::paths::generated::RepoRoot;
use crate::project::Context;
use crate::project::IsArtifact;
use crate::project::IsTarget;
use crate::source::BuildTargetJob;
use crate::source::WithDestination;
use futures_util::future::try_join;

#[derive(Debug, Clone, PartialEq)]
pub struct Artifact {
    /// Directory with unpacked client distribution.
    pub unpacked:            PathBuf,
    /// Entry point within an unpacked client distribution.
    pub unpacked_executable: PathBuf,
}

impl AsRef<Path> for Artifact {
    fn as_ref(&self) -> &Path {
        &self.unpacked
    }
}

impl IsArtifact for Artifact {}

impl Artifact {
    pub fn new(target_os: OS, target_arch: Arch, dist_dir: impl AsRef<Path>) -> Self {
        let unpacked = dist_dir.as_ref().join(match target_os {
            OS::Linux => "linux-unpacked",
            OS::MacOS if target_arch == Arch::AArch64 => "mac-arm64",
            OS::MacOS if target_arch == Arch::X86_64 => "mac",
            OS::Windows => "win-unpacked",
            _ => todo!("{target_os}-{target_arch} combination is not supported"),
        });
        let unpacked_executable = match target_os {
            OS::Linux => "enso",
            OS::MacOS => "Enso.app",
            OS::Windows => "Enso.exe",
            _ => todo!("{target_os}-{target_arch} combination is not supported"),
        }
        .into();
        Self { unpacked, unpacked_executable }
    }

    // pub async fn upload_as_ci_artifact(&self) -> Result {
    //     if is_in_env() {
    //         upload_compressed_directory(&self.unpacked, format!("ide-unpacked-{}", TARGET_OS))
    //             .await?;
    //     } else {
    //         info!("Not in the CI environment, will not upload the artifacts.")
    //     }
    //     Ok(())
    // }
}

#[derive(derivative::Derivative)]
#[derivative(Debug)]
pub struct BuildInput {
    #[derivative(Debug(format_with = "std::fmt::Display::fmt"))]
    pub repo_root:       RepoRoot,
    #[derivative(Debug(format_with = "std::fmt::Display::fmt"))]
    pub version:         Version,
    #[derivative(Debug = "ignore")]
    pub project_manager: BoxFuture<'static, Result<crate::project::backend::Artifact>>,
    #[derivative(Debug = "ignore")]
    pub gui:             BoxFuture<'static, Result<crate::project::gui::Artifact>>,
}

#[derive(Clone, Copy, Debug)]
pub struct Ide {
    pub target_os:   OS,
    pub target_arch: Arch,
}

impl Ide {
    pub fn build(
        &self,
        input: BuildInput,
        output_path: impl AsRef<Path> + Send + Sync + 'static,
    ) -> BoxFuture<'static, Result<Artifact>> {
        let BuildInput { repo_root, version, project_manager, gui } = input;
        let ide_desktop = crate::ide::web::IdeDesktop::new(&repo_root.app.ide_desktop);
        let target_os = self.target_os;
        let target_arch = self.target_arch;
        async move {
            let (gui, project_manager) = try_join(gui, project_manager).await?;
            ide_desktop.dist(&gui, &project_manager, &output_path, target_os, false).await?;
            Ok(Artifact::new(target_os, target_arch, output_path))
        }
        .boxed()
    }
}

impl IsTarget for Ide {
    type BuildInput = BuildInput;
    type Artifact = Artifact;

    fn artifact_name(&self) -> String {
        format!("ide-unpacked-{}", TARGET_OS)
    }

    fn adapt_artifact(self, path: impl AsRef<Path>) -> BoxFuture<'static, Result<Self::Artifact>> {
        let ret = Artifact::new(self.target_os, self.target_arch, path);
        ready(Ok(ret)).boxed()
    }

    fn build_internal(
        &self,
        context: Context,
        job: BuildTargetJob<Self>,
    ) -> BoxFuture<'static, Result<Self::Artifact>> {
        let WithDestination { inner, destination } = job;
        self.build(inner, destination)
    }
}
