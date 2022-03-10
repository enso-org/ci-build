use crate::prelude::*;

use crate::args::default_repo;
use crate::args::Args;
use crate::args::BuildKind;
use crate::args::WhatToDo;
use crate::paths::ComponentPaths;
use crate::paths::Paths;
use crate::version::Versions;

use anyhow::Context;
use ide_ci::env::Variable;
use ide_ci::extensions::path::PathExt;
use ide_ci::future::AsyncPolicy;
use ide_ci::goodies::graalvm;
use ide_ci::models::config::RepoContext;
use platforms::TARGET_OS;
use std::env::consts::EXE_EXTENSION;

pub mod context;
pub mod env;
pub mod sbt;

const FLATC_VERSION: Version = Version::new(1, 12, 0);
// const GRAAL_VERSION: Version = Version::new(21, 1, 0);
// const GRAAL_JAVA_VERSION: graalvm::JavaVersion = graalvm::JavaVersion::Java11;
const PARALLEL_ENSO_TESTS: AsyncPolicy = AsyncPolicy::Sequential;

pub async fn download_project_templates(client: reqwest::Client, enso_root: PathBuf) -> Result {
    // Download Project Template Files
    let output_base = enso_root.join("lib/scala/pkg/src/main/resources/");
    let url_base = Url::parse("https://github.com/enso-org/project-templates/raw/main/")?;
    let to_handle = [
        ("Orders", vec!["data/store_data.xlsx", "src/Main.enso"]),
        ("Restaurants", vec!["data/la_districts.csv", "data/restaurants.csv", "src/Main.enso"]),
        ("Stargazers", vec!["src/Main.enso"]),
    ];

    let mut futures = Vec::<BoxFuture<'static, Result>>::new();
    for (project_name, relative_paths) in to_handle {
        for relative_path in relative_paths {
            let relative_url_base = url_base.join(&format!("{}/", project_name))?;
            let relative_output_base = output_base.join(project_name.to_lowercase());
            let client = client.clone();
            let future = async move {
                ide_ci::io::download_relative(
                    &client,
                    &relative_url_base,
                    &relative_output_base,
                    &PathBuf::from(relative_path),
                )
                .await?;
                Ok(())
            };
            futures.push(future.boxed());
        }
    }

    let _result = ide_ci::future::try_join_all(futures, AsyncPolicy::FutureParallelism).await?;
    println!("Completed downloading templates");
    Ok(())
}

#[derive(Clone, Copy, Debug, Display, PartialEq)]
pub enum BuildMode {
    Development,
    NightlyRelease,
}

#[derive(Clone, Copy, Debug)]
pub struct BuildConfiguration {
    /// If true, repository shall be cleaned at the build start.
    ///
    /// Makes sense given that incremental builds with SBT are currently broken.
    pub clean_repo:            bool,
    pub mode:                  BuildMode,
    pub test_scala:            bool,
    pub test_standard_library: bool,
    /// Whether benchmarks are compiled.
    ///
    /// Note that this does not run the benchmarks, only ensures that they are buildable.
    pub benchmark_compilation: bool,
    pub build_js_parser:       bool,
    pub build_bundles:         bool,
}

impl BuildConfiguration {
    pub fn new(args: &Args) -> Self {
        let mut config = match args.kind {
            BuildKind::Dev => DEV,
            BuildKind::Nightly => NIGHTLY,
        };

        // Update build configuration with a custom arg overrides.
        if matches!(args.command, WhatToDo::Upload(_)) || args.bundle.contains(&true) {
            config.build_bundles = true;
        }
        config
    }
}

pub const DEV: BuildConfiguration = BuildConfiguration {
    clean_repo:            true,
    mode:                  BuildMode::Development,
    test_scala:            true,
    test_standard_library: true,
    benchmark_compilation: true,
    build_js_parser:       true,
    build_bundles:         false,
};

pub const NIGHTLY: BuildConfiguration = BuildConfiguration {
    clean_repo:            true,
    mode:                  BuildMode::NightlyRelease,
    test_scala:            false,
    test_standard_library: false,
    benchmark_compilation: false,
    build_js_parser:       false,
    build_bundles:         false,
};

#[derive(Clone, PartialEq, Debug)]
pub enum ReleaseCommand {
    Create,
    Upload,
    Publish,
}

impl TryFrom<WhatToDo> for ReleaseCommand {
    type Error = anyhow::Error;

    fn try_from(value: WhatToDo) -> Result<Self> {
        Ok(match value {
            WhatToDo::Create(_) => ReleaseCommand::Create,
            WhatToDo::Upload(_) => ReleaseCommand::Upload,
            WhatToDo::Publish(_) => ReleaseCommand::Publish,
            _ => bail!("Not a release command: {}", value),
        })
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct ReleaseOperation {
    pub command: ReleaseCommand,
    pub repo:    RepoContext,
}

impl ReleaseOperation {
    pub fn new(args: &Args) -> Result<Self> {
        let command = args.command.clone().try_into()?;
        let repo = match args.repo.clone() {
            Some(repo) => repo,
            None => ide_ci::actions::env::Repository.fetch()?,
        };

        Ok(Self { command, repo })
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct RunOperation {
    pub command_pieces: Vec<OsString>,
}

impl RunOperation {}


#[derive(Clone, PartialEq, Debug)]
pub struct BuildOperation {}

impl BuildOperation {}

#[derive(Clone, PartialEq, Debug)]
pub enum Operation {
    Release(ReleaseOperation),
    Run(RunOperation),
    Build(BuildOperation),
}

#[derive(Clone, PartialEq, Debug, Default)]
pub struct BuiltArtifacts {
    pub bundles:  Vec<PathBuf>,
    pub packages: Vec<PathBuf>,
}

#[context("Failed to create a launcher distribution.")]
pub fn create_launcher_distribution(paths: &Paths) -> Result {
    paths.launcher.clear()?;
    ide_ci::io::copy_to(
        paths.repo_root.join_many(["distribution", "launcher", "THIRD-PARTY"]),
        &paths.launcher.dir,
    )?;
    ide_ci::io::copy_to(
        paths.repo_root.join("enso").with_extension(EXE_EXTENSION),
        &paths.launcher.dir.join("bin"),
    )?;
    //     IO.createDirectory(distributionRoot / "dist")
    //     IO.createDirectory(distributionRoot / "runtime")
    for filename in [".enso.portable", "README.md"] {
        ide_ci::io::copy_to(
            paths.repo_root.join_many(["distribution", "launcher", filename]),
            &paths.launcher.dir,
        )?;
    }
    Ok(())
}

pub async fn create_packages(paths: &Paths) -> Result<Vec<PathBuf>> {
    let mut ret = Vec::new();
    if paths.launcher.root.exists() {
        println!("Packaging launcher.");
        ret.push(package_component(&paths.launcher).await?);
        // IO.createDirectories(
        //     Seq("dist", "config", "runtime").map(root / "enso" / _)
        // )
    }

    if paths.engine.root.exists() {
        println!("Packaging engine.");
        ret.push(package_component(&paths.engine).await?);
    }
    Ok(ret)
}

#[context("Placing a GraalVM package under {}", target_directory.as_ref().display())]
pub async fn place_graal_under(target_directory: impl AsRef<Path>) -> Result {
    let graal_path = PathBuf::from(ide_ci::env::expect_var_os("JAVA_HOME")?);
    let graal_dirname = graal_path
        .file_name()
        .context(anyhow!("Invalid Graal Path deduced from JAVA_HOME: {}", graal_path.display()))?;
    ide_ci::io::mirror_directory(&graal_path, target_directory.as_ref().join(graal_dirname)).await
}

#[context("Placing a Enso Engine package in {}", target_engine_dir.as_ref().display())]
pub fn place_component_at(
    engine_paths: &ComponentPaths,
    target_engine_dir: impl AsRef<Path>,
) -> Result {
    ide_ci::io::copy(&engine_paths.dir, &target_engine_dir)
}

#[async_trait]
trait ComponentPathExt {
    async fn pack(&self) -> Result;
    fn clear(&self) -> Result;
}

#[async_trait]
impl ComponentPathExt for ComponentPaths {
    async fn pack(&self) -> Result {
        ide_ci::archive::create(&self.artifact_archive, [&self.dir]).await
    }
    fn clear(&self) -> Result {
        ide_ci::io::remove_dir_if_exists(&self.root)?;
        ide_ci::io::remove_file_if_exists(&self.artifact_archive)
    }
}

pub async fn create_bundle(
    paths: &Paths,
    base_component: &ComponentPaths,
    bundle: &ComponentPaths,
) -> Result {
    bundle.clear()?;
    ide_ci::io::copy(&base_component.root, &bundle.root)?;

    let bundled_engine_dir = bundle.dir.join("dist").join(paths.version().to_string());
    place_component_at(&paths.engine, &bundled_engine_dir)?;
    place_graal_under(bundle.dir.join("runtime")).await?;
    Ok(())
}

pub async fn create_bundles(paths: &Paths) -> Result<Vec<PathBuf>> {
    // Make sure that Graal has the needed optional components installed (on platforms that support
    // them).
    if TARGET_OS != OS::Windows {
        // Windows does not support sulong.
        graalvm::Gu.call_args(["install", "python", "r"]).await?;
    }

    // Launcher bundle.
    let engine_bundle =
        ComponentPaths::new(&paths.build_dist_root, "enso-bundle", "enso", &paths.triple);
    create_bundle(paths, &paths.launcher, &engine_bundle).await?;
    engine_bundle.pack().await?;


    // Project manager bundle.
    let pm_bundle = ComponentPaths::new(
        &paths.build_dist_root,
        "project-manager-bundle",
        "enso",
        &paths.triple,
    );
    create_bundle(paths, &paths.project_manager, &pm_bundle).await?;
    ide_ci::io::copy(
        paths.repo_root.join_many(["distribution", "enso.bundle.template"]),
        pm_bundle.dir.join(".enso.bundle"),
    )?;
    pm_bundle.pack().await?;
    Ok(vec![engine_bundle.artifact_archive, pm_bundle.artifact_archive])
}

pub async fn package_component(paths: &ComponentPaths) -> Result<PathBuf> {
    #[cfg(not(target_os = "windows"))]
    {
        let pattern =
            paths.dir.join_many(["bin", "*"]).with_extension(EXE_EXTENSION).display().to_string();
        for binary in glob::glob(&pattern)? {
            ide_ci::io::allow_owner_execute(binary?)?;
        }
    }

    ide_ci::archive::create(&paths.artifact_archive, [&paths.root]).await?;
    Ok(paths.artifact_archive.clone())
}
