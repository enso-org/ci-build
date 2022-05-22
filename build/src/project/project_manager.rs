use crate::prelude::*;

use crate::engine::BuildConfiguration;
use crate::engine::BuildOperation;
use crate::project::IsArtifact;
use crate::project::IsTarget;
use crate::version::Versions;

use crate::paths::pretty_print_arch;
use anyhow::Context;
use derivative::Derivative;
use ide_ci::archive::is_archive_name;
use ide_ci::extensions::os::OsExt;
use ide_ci::goodie::GoodieDatabase;
use octocrab::models::repos::Asset;

#[derive(Clone, Derivative)]
#[derivative(Debug)]
pub struct BuildInput {
    pub repo_root: PathBuf,
    pub versions:  Versions,
    /// Used for GraalVM release lookup.
    ///
    /// Default instance will suffice, but then we are prone to hit API limits. Authorized one will
    /// likely do better.
    #[derivative(Debug = "ignore")]
    pub octocrab:  Octocrab,
}

#[derive(Clone, Debug)]
pub struct Artifact {
    /// Location of the Project Manager distribution.
    pub path:            crate::paths::generated::ProjectManager,
    /// Versions of Engine that are bundled in this Project Manager distribution.
    ///
    /// Technically a Project Manager bundle can be shipped with arbitrary number of Enso Engine
    /// packages. However in packages we create it is almost always zero (for plain PM package) or
    /// one (for full PM bundle).
    ///
    /// Artifacts built with [`ProjectManager::build_locally`] will have exactly one engine
    /// bundled.
    pub engine_versions: Vec<Version>,
}

impl AsRef<Path> for Artifact {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl IsArtifact for Artifact {}

/// Retrieves a list of all Enso Engine versions that are bundled within a given Project Manager
/// distribution.
#[context("Failed to list bundled engine versions: {}", project_manager_bundle)]
pub async fn bundled_engine_versions(
    project_manager_bundle: &crate::paths::generated::ProjectManager,
) -> Result<Vec<Version>> {
    let mut ret = vec![];

    let mut dir_reader = ide_ci::fs::tokio::read_dir(&project_manager_bundle.dist).await?;
    while let Some(entry) = dir_reader.next_entry().await? {
        if entry.metadata().await?.is_dir() {
            ret.push(Version::from_str(entry.file_name().as_str())?);
        }
    }
    Ok(ret)
}

#[derive(Clone, Debug)]
pub struct ProjectManager {
    pub target_os: OS,
}

impl ProjectManager {
    pub fn matches_platform(&self, name: &str) -> bool {
        // Sample name: "project-manager-bundle-2022.1.1-nightly.2022-04-16-linux-amd64.tar.gz"
        name.contains(self.target_os.as_str()) && name.contains(pretty_print_arch(TARGET_ARCH))
        // TODO workaround for macOS and M1 (they should be allowed to use amd64 artifacts)
    }
}

impl IsTarget for ProjectManager {
    type BuildInput = BuildInput;
    type Artifact = Artifact;

    fn artifact_name(&self) -> String {
        // Version is not part of the name intentionally. We want to refer to PM bundles as
        // artifacts without knowing their version.
        format!("project-manager-{}", self.target_os)
    }

    fn adapt_artifact(self, path: impl AsRef<Path>) -> BoxFuture<'static, Result<Self::Artifact>> {
        let path = crate::paths::generated::ProjectManager::new(
            path.as_ref(),
            self.target_os.exe_suffix(),
        );
        async move {
            let engine_versions = bundled_engine_versions(&path).await?;
            Ok(Artifact { path, engine_versions })
        }
        .boxed()
    }

    fn build_locally(
        &self,
        input: Self::BuildInput,
        output_path: impl AsRef<Path> + Send + Sync + 'static,
    ) -> BoxFuture<'static, Result<Self::Artifact>> {
        let target_os = self.target_os;
        let this = self.clone();
        async move {
            ensure!(
                target_os == TARGET_OS,
                "Enso Project Manager cannot be built on '{target_os}' for target '{TARGET_OS}'.",
            );
            let paths =
                crate::paths::Paths::new_versions(&input.repo_root, input.versions.clone())?;
            let context = crate::engine::context::RunContext {
                operation: crate::engine::Operation::Build(BuildOperation {}),
                goodies: GoodieDatabase::new()?,
                config: BuildConfiguration {
                    clean_repo: false,
                    build_project_manager_bundle: true,
                    ..crate::engine::NIGHTLY
                },
                octocrab: input.octocrab.clone(),
                paths,
            };
            let artifacts = context.build().await?;
            let project_manager =
                artifacts.bundles.project_manager.context("Missing project manager bundle!")?;
            ide_ci::fs::mirror_directory(&project_manager.dir, &output_path).await?;
            this.adapt_artifact(output_path).await
            // Artifact::from_existing(output_path.as_ref()).await
        }
        .boxed()
    }

    fn find_asset(&self, assets: Vec<Asset>) -> Result<Asset> {
        assets
            .into_iter()
            .find(|asset| {
                let name = &asset.name;
                self.matches_platform(name)
                    && is_archive_name(name)
                    && name.contains("project-manager")
            })
            .context("Failed to find release asset with Enso Project Manager bundle.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use ide_ci::log::setup_logging;

    #[tokio::test]
    async fn run_project_manager() -> Result {
        setup_logging()?;
        let pm = PathBuf::from(r"H:\NBO\ci-build\dist\project-manager\bin\project-manager.exe");
        Command::new(pm).run_ok().await?;
        Ok(())
    }
}
