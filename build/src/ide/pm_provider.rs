use crate::engine::BuildConfiguration;
use crate::engine::BuildOperation;
// use crate::paths::generated::Paths;
use crate::paths::TargetTriple;
use crate::prelude::*;
use crate::setup_octocrab;
use crate::version::Versions;
use anyhow::Context;
use ide_ci::goodie::GoodieDatabase;
#[derive(Clone, Debug)]
pub enum ProjectManagerSource {
    Local {
        repo_root: PathBuf,
    },
    /// Wraps path to a Project Manager bundle.
    ///
    /// Such path looks like:
    /// ```text
    /// H:\NBO\enso5\built-distribution\project-manager-bundle-2022.1.1-dev-windows-amd64\enso
    /// ```
    Bundle(PathBuf),
    Release(Version),
}

pub struct ProjectManagerArtifacts(pub PathBuf);

impl ProjectManagerSource {
    pub async fn get(
        &self,
        triple: TargetTriple,
        output_path: impl AsRef<Path>,
    ) -> Result<ProjectManagerArtifacts> {
        let output_path = output_path.as_ref();
        match self {
            ProjectManagerSource::Local { repo_root } => {
                let paths =
                    crate::paths::Paths::new_version(&repo_root, triple.versions.version.clone())?;
                let context = crate::engine::context::RunContext {
                    operation: crate::engine::Operation::Build(BuildOperation {}),
                    goodies: GoodieDatabase::new()?,
                    config: BuildConfiguration {
                        clean_repo: false,
                        build_project_manager_bundle: true,
                        ..crate::engine::NIGHTLY
                    },
                    octocrab: setup_octocrab()?,
                    paths,
                };
                let artifacts = context.build().await?;
                let project_manager =
                    artifacts.bundles.project_manager.context("Missing project manager bundle!")?;
                ide_ci::fs::mirror_directory(&project_manager.dir, &output_path).await?;
            }
            ProjectManagerSource::Bundle(path) => {
                assert_eq!(path.file_name(), Some(OsStr::new("enso")));
                ide_ci::fs::mirror_directory(&path, &output_path).await?;
            }
            ProjectManagerSource::Release(version) => {
                todo!();
                let needed_target = TargetTriple::new(Versions::new(version.clone()));
                crate::project_manager::ensure_present(&output_path, &needed_target).await?;
            }
        };
        Ok(ProjectManagerArtifacts(output_path.to_path_buf()))
    }
}
