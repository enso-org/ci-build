use crate::prelude::*;
use std::env::consts::EXE_SUFFIX;
use std::lazy::Lazy;
use std::lazy::SyncLazy;

use crate::engine::BuildConfiguration;
use crate::engine::BuildOperation;
use anyhow::Context;
use ide_ci::goodie::GoodieDatabase;
use ide_ci::program::version::find_in_text;
use platforms::TARGET_OS;

use crate::project::IsArtifact;
use crate::project::IsTarget;
use crate::project::Source;
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
    pub path:     crate::paths::generated::ProjectManager,
    pub versions: Versions,
}

impl AsRef<Path> for Artifact {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl IsArtifact for Artifact {
    fn from_existing(path: impl AsRef<Path>) -> BoxFuture<'static, Result<Self>> {
        let path = crate::paths::generated::ProjectManager::new(path.as_ref(), EXE_SUFFIX);
        async move {
            let output =
                Command::new(&path.bin.project_managerexe).arg("--version").output_ok().await?;
            let string = String::from_utf8(output.stdout)?;
            let version = find_in_text(&string)?;
            Ok(Self { path, versions: Versions::new(version) })
        }
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn build(
        &self,
        input: Self::BuildInput,
        output_path: impl AsRef<Path> + Send + Sync + 'static,
    ) -> BoxFuture<'static, Result<Self::Output>> {
        async move {
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
            Artifact::from_existing(output_path.as_ref()).await
        }
        .boxed()
    }
}
//
// pub enum ProjectManagerSource {
//     Local {
//         repo_root: PathBuf,
//     },
//     /// Wraps path to a Project Manager bundle.
//     ///
//     /// Such path looks like:
//     /// ```text
//     /// H:\NBO\enso5\built-distribution\project-manager-bundle-2022.1.1-dev-windows-amd64\enso
//     /// ```
//     Bundle(PathBuf),
//     Release(Version),
// }
//
// pub struct ProjectManagerArtifacts(pub PathBuf);

//
//
// impl ProjectManagerSource {
//     pub async fn get(
//         &self,
//         triple: TargetTriple,
//         output_path: impl AsRef<Path>,
//     ) -> Result<ProjectManagerArtifacts> {
//         let output_path = output_path.as_ref();
//         match self {
//             ProjectManagerSource::Local { repo_root } => {
//                 let paths =
//                     crate::paths::Paths::new_version(&repo_root,
// triple.versions.version.clone())?;                 let context =
// crate::engine::context::RunContext {                     operation:
// crate::engine::Operation::Build(BuildOperation {}),                     goodies:
// GoodieDatabase::new()?,                     config: BuildConfiguration {
//                         clean_repo: false,
//                         build_project_manager_bundle: true,
//                         ..crate::engine::NIGHTLY
//                     },
//                     octocrab: setup_octocrab()?,
//                     paths,
//                 };
//                 let artifacts = context.build().await?;
//                 let project_manager =
//                     artifacts.bundles.project_manager.context("Missing project manager
// bundle!")?;                 ide_ci::fs::mirror_directory(&project_manager.dir,
// &output_path).await?;             }
//             ProjectManagerSource::Bundle(path) => {
//                 assert_eq!(path.file_name(), Some(OsStr::new("enso")));
//                 ide_ci::fs::mirror_directory(&path, &output_path).await?;
//             }
//             ProjectManagerSource::Release(version) => {
//                 todo!();
//                 let needed_target = TargetTriple::new(Versions::new(version.clone()));
//                 crate::project_manager::ensure_present(&output_path, &needed_target).await?;
//             }
//         };
//         Ok(ProjectManagerArtifacts(output_path.to_path_buf()))
//     }
// }
