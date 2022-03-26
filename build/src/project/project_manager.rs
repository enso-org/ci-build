use crate::prelude::*;
use std::lazy::Lazy;
use std::lazy::SyncLazy;

use crate::engine::BuildConfiguration;
use crate::engine::BuildOperation;
use anyhow::Context;
use ide_ci::goodie::GoodieDatabase;
use platforms::TARGET_OS;

use crate::project::IsTarget;
use crate::version::Versions;

#[derive(Clone, Debug)]
pub struct Inputs {
    pub repo_root: PathBuf,
    pub versions:  Versions,
    /// Necessary for GraalVM lookup.
    pub octocrab:  Octocrab,
}

#[derive(Clone, Debug)]
pub struct Artifact {
    pub path:     PathBuf,
    pub versions: Versions,
}

impl AsRef<Path> for Artifact {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}


impl From<&Path> for Artifact {
    fn from(path: &Path) -> Self {
        todo!()
    }
}

#[derive(Clone, Debug)]
pub struct ProjectManager;

#[async_trait]
impl IsTarget for ProjectManager {
    type BuildInput = Inputs;
    type Output = Artifact;

    fn artifact_name(&self) -> &str {
        // Version is not part of the name intentionally. We want to refer to PM bundles as
        // artifacts without knowing their version.
        static NAME: SyncLazy<String> = SyncLazy::new(|| format!("project-manager-{}", TARGET_OS));
        &*NAME
    }

    async fn build(
        &self,
        input: Self::BuildInput,
        output_path: impl AsRef<Path> + Send + Sync + 'static,
    ) -> Result<Self::Output> {
        let paths = crate::paths::Paths::new_versions(&input.repo_root, input.versions.clone())?;
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
        todo!()
    }
}
