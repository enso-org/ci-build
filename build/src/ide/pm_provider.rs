use crate::engine::BuildConfiguration;
use crate::engine::BuildOperation;
use crate::paths::generated::Paths;
use crate::paths::TargetTriple;
use crate::prelude::*;
use crate::setup_octocrab;
use crate::version::Versions;
use ide_ci::goodie::GoodieDatabase;

#[derive(Clone, Debug)]
pub enum ProjectManagerSource {
    Local,
    Bundle(PathBuf),
    Release(Version),
}

impl ProjectManagerSource {
    pub async fn get(&self, paths: Paths, triple: TargetTriple) -> Result {
        let repo_root = &paths.repo_root;
        let target_path = &paths.repo_root.dist.project_manager;
        match self {
            ProjectManagerSource::Local => {
                let context = crate::engine::context::RunContext {
                    operation: crate::engine::Operation::Build(BuildOperation {}),
                    goodies:   GoodieDatabase::new()?,
                    config:    BuildConfiguration {
                        clean_repo: false,
                        build_bundles: true,
                        ..crate::engine::NIGHTLY
                    },
                    octocrab:  setup_octocrab()?,
                    paths:     crate::paths::Paths::new_version(
                        &repo_root.path,
                        triple.versions.version.clone(),
                    )?,
                };
                dbg!(context.execute().await?);
                ide_ci::fs::reset_dir(&target_path)?;
                ide_ci::fs::copy_to(
                    &repo_root.built_distribution.project_manager_bundle_triple.enso,
                    &target_path,
                )?;
            }
            ProjectManagerSource::Bundle(path) => {
                assert_eq!(path.file_name().and_then(|f| f.to_str()), Some("enso"));
                ide_ci::fs::reset_dir(&target_path)?;
                ide_ci::fs::copy_to(&path, &target_path)?;
            }
            ProjectManagerSource::Release(version) => {
                let needed_target = TargetTriple::new(Versions::new(version.clone()));
                crate::project_manager::ensure_present(&target_path, &needed_target).await?;
            }
        };
        Ok(())
    }
}
