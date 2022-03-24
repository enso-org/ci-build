use crate::prelude::*;

use crate::args::Args;
use crate::args::BuildKind;
use crate::args::WhatToDo;
use crate::paths::ComponentPaths;
use crate::paths::Paths;

use ide_ci::env::Variable;
use ide_ci::future::AsyncPolicy;
use ide_ci::models::config::RepoContext;
use platforms::TARGET_OS;

pub mod bundle;
pub mod context;
pub mod env;
pub mod sbt;

pub use context::RunContext;

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
    pub clean_repo: bool,
    pub mode: BuildMode,
    pub test_scala: bool,
    pub test_standard_library: bool,
    /// Whether benchmarks are compiled.
    ///
    /// Note that this does not run the benchmarks, only ensures that they are buildable.
    pub benchmark_compilation: bool,
    pub build_js_parser: bool,
    pub build_engine_package: bool,
    pub build_launcher_package: bool,
    pub build_project_manager_package: bool,
    pub build_launcher_bundle: bool,
    pub build_project_manager_bundle: bool,
}

impl BuildConfiguration {
    pub fn new(args: &Args) -> Self {
        let mut config = match args.kind {
            BuildKind::Dev => DEV,
            BuildKind::Nightly => NIGHTLY,
        };

        // Update build configuration with a custom arg overrides.
        if matches!(args.command, WhatToDo::Upload(_)) || args.bundle.contains(&true) {
            config.build_launcher_bundle = true;
            config.build_project_manager_bundle = true;
        }

        if config.build_launcher_bundle {
            config.build_launcher_package = true;
            config.build_engine_package = true;
        }

        if config.build_project_manager_bundle {
            config.build_project_manager_package = true;
            config.build_engine_package = true;
        }

        config
    }

    pub fn build_engine_package(&self) -> bool {
        self.build_engine_package || self.build_launcher_bundle || self.build_project_manager_bundle
    }

    pub fn build_project_manager_package(&self) -> bool {
        self.build_project_manager_package || self.build_project_manager_bundle
    }

    pub fn build_launcher_package(&self) -> bool {
        self.build_launcher_package || self.build_launcher_bundle
    }
}

pub const DEV: BuildConfiguration = BuildConfiguration {
    clean_repo: true,
    mode: BuildMode::Development,
    test_scala: true,
    test_standard_library: true,
    benchmark_compilation: true,
    build_js_parser: matches!(TARGET_OS, OS::Linux),
    build_engine_package: false,
    build_launcher_package: false,
    build_project_manager_package: false,
    build_launcher_bundle: false,
    build_project_manager_bundle: false,
};

pub const NIGHTLY: BuildConfiguration = BuildConfiguration {
    clean_repo: true,
    mode: BuildMode::NightlyRelease,
    test_scala: false,
    test_standard_library: false,
    benchmark_compilation: false,
    build_js_parser: false,
    build_engine_package: false,
    build_launcher_package: false,
    build_project_manager_package: false,
    build_launcher_bundle: false,
    build_project_manager_bundle: false,
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
    pub packages: BuiltPackageArtifacts,
    pub bundles:  BuiltBundleArtifacts,
}

#[derive(Clone, PartialEq, Debug, Default)]
pub struct BuiltPackageArtifacts {
    pub engine:          Option<ComponentPaths>,
    pub launcher:        Option<ComponentPaths>,
    pub project_manager: Option<ComponentPaths>,
}

impl BuiltPackageArtifacts {
    pub fn iter(&self) -> impl IntoIterator<Item = &ComponentPaths> {
        [&self.engine, &self.launcher, &self.project_manager]
            .into_iter()
            .map(|b| b.iter())
            .flatten()
    }
}

#[derive(Clone, PartialEq, Debug, Default)]
pub struct BuiltBundleArtifacts {
    pub launcher:        Option<ComponentPaths>,
    pub project_manager: Option<ComponentPaths>,
}

impl BuiltBundleArtifacts {
    pub fn iter(&self) -> impl IntoIterator<Item = &ComponentPaths> {
        [&self.project_manager, &self.launcher].into_iter().map(|b| b.iter()).flatten()
    }
}

pub async fn create_packages(paths: &Paths) -> Result<Vec<PathBuf>> {
    let mut ret = Vec::new();
    if paths.launcher.root.exists() {
        println!("Packaging launcher.");
        ret.push(package_component(&paths.launcher).await?);
    }
    Ok(ret)
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
        ide_ci::fs::remove_dir_if_exists(&self.root)?;
        ide_ci::fs::remove_file_if_exists(&self.artifact_archive)
    }
}

pub async fn package_component(paths: &ComponentPaths) -> Result<PathBuf> {
    #[cfg(not(target_os = "windows"))]
    {
        let pattern =
            paths.dir.join_many(["bin", "*"]).with_extension(EXE_EXTENSION).display().to_string();
        for binary in glob::glob(&pattern)? {
            ide_ci::fs::allow_owner_execute(binary?)?;
        }
    }

    ide_ci::archive::create(&paths.artifact_archive, [&paths.root]).await?;
    Ok(paths.artifact_archive.clone())
}
