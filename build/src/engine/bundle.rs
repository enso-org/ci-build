use crate::prelude::*;

use crate::engine::ComponentPathExt;
use crate::paths::ComponentPaths;
use crate::paths::Paths;
use anyhow::Context;

#[async_trait]
pub trait Bundle {
    const PREFIX: &'static str;
    const DIRNAME: &'static str;

    fn base_distribution(paths: &Paths) -> &ComponentPaths;

    fn suggest_paths(paths: &Paths) -> ComponentPaths {
        ComponentPaths::new(&paths.build_dist_root, Self::PREFIX, Self::DIRNAME, &paths.triple)
    }

    async fn create(paths: &Paths) -> Result<ComponentPaths> {
        let bundle = Self::suggest_paths(paths);

        bundle.clear()?;

        let base_component = Self::base_distribution(paths);
        ide_ci::fs::copy(&base_component.root, &bundle.root)?;

        // Add engine.
        let bundled_engine_dir = bundle.dir.join("dist").join(paths.version().to_string());
        place_component_at(&paths.engine, &bundled_engine_dir).await?;

        // Add GraalVM runtime.
        place_graal_under(bundle.dir.join("runtime")).await?;

        // Add portable distribution marker.
        ide_ci::fs::copy(
            paths.repo_root.join_iter(["distribution", "enso.bundle.template"]),
            bundle.dir.join(".enso.bundle"),
        )?;
        Ok(bundle)
    }
}

pub struct Launcher;
impl Bundle for Launcher {
    const PREFIX: &'static str = "enso-bundle";
    const DIRNAME: &'static str = "enso";
    fn base_distribution(paths: &Paths) -> &ComponentPaths {
        &paths.launcher
    }
}

pub struct ProjectManager;
impl Bundle for ProjectManager {
    const PREFIX: &'static str = "project-manager-bundle";
    const DIRNAME: &'static str = "enso";
    fn base_distribution(paths: &Paths) -> &ComponentPaths {
        &paths.project_manager
    }
}

#[context("Placing a GraalVM package under {}", target_directory.as_ref().display())]
pub async fn place_graal_under(target_directory: impl AsRef<Path>) -> Result {
    let graal_path = PathBuf::from(ide_ci::env::expect_var_os("JAVA_HOME")?);
    let graal_dirname = graal_path
        .file_name()
        .context(anyhow!("Invalid Graal Path deduced from JAVA_HOME: {}", graal_path.display()))?;
    ide_ci::fs::mirror_directory(&graal_path, target_directory.as_ref().join(graal_dirname)).await
}

#[context("Placing a Enso Engine package in {}", target_engine_dir.as_ref().display())]
pub async fn place_component_at(
    engine_paths: &ComponentPaths,
    target_engine_dir: impl AsRef<Path>,
) -> Result {
    ide_ci::fs::mirror_directory(&engine_paths.dir, &target_engine_dir).await
}
