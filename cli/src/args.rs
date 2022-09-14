use crate::prelude::*;

use clap::Args;
use clap::Parser;
use clap::Subcommand;
use enso_build::engine::BuildConfigurationResolved;
use enso_build::engine::BuildOperation;
use enso_build::engine::Operation;
use enso_build::engine::ReleaseCommand;
use enso_build::engine::ReleaseOperation;
use enso_build::engine::RunContext;
use enso_build::engine::RunOperation;
use enso_build::engine::DEV;
use enso_build::engine::NIGHTLY;
use enso_build::setup_octocrab;
use enso_build::version::deduce_versions;
use enso_build::version::BuildKind;

use enso_build::paths::Paths;
use ide_ci::cache::Cache;
use ide_ci::env::Variable;
use ide_ci::goodie::GoodieDatabase;
use ide_ci::models::config::RepoContext;

#[derive(Subcommand, Clone, PartialEq, Eq, Debug, strum::Display)]
pub enum WhatToDo {
    Build(Build),
    // Three release-related commands below.
    Upload(UploadAsset),
    // Utilities
    Run(Run),
}

impl TryFrom<WhatToDo> for ReleaseCommand {
    type Error = anyhow::Error;

    fn try_from(value: WhatToDo) -> Result<Self> {
        Ok(match value {
            WhatToDo::Upload(_) => ReleaseCommand::Upload,
            _ => bail!("Not a release command: {}", value),
        })
    }
}

impl WhatToDo {
    pub fn is_release_command(&self) -> bool {
        use WhatToDo::*;
        // Not using matches! to force manual check when extending enum.
        match self {
            Upload(_) => true,
            Build(_) | Run(_) => false,
        }
    }
}

/// Just build the packages.
#[derive(Args, Clone, PartialEq, Eq, Debug)]
pub struct Build {}

/// Create a new draft release on GitHub and emit relevant information to the CI environment.
#[derive(Args, Clone, PartialEq, Eq, Debug)]
pub struct CreateRelease {}

/// Build all the release packages and bundles and upload them to GitHub release. Must run with
/// environment adjusted by the `prepare` command.
#[derive(Args, Clone, PartialEq, Eq, Debug)]
pub struct UploadAsset {}

/// Publish the release.  Must run with environment adjusted by the `prepare` command. Typically
/// called once after platform-specific `upload` commands are done.
#[derive(Args, Clone, PartialEq, Eq, Debug)]
pub struct PublishRelease {}

/// Run an arbitrary command with the build environment set (like `PATH`).
#[derive(Args, Clone, PartialEq, Eq, Debug)]
pub struct Run {
    #[clap(last = true)]
    pub command_pieces: Vec<OsString>,
}

pub fn default_kind() -> BuildKind {
    crate::BuildKind.fetch().unwrap_or(BuildKind::Dev)
}

pub fn default_repo() -> Option<RepoContext> {
    ide_ci::actions::env::GITHUB_REPOSITORY.get().ok()
}

pub fn parse_repo_context(value: &str) -> std::result::Result<Option<RepoContext>, String> {
    RepoContext::from_str(value).map(Some).map_err(|e| e.to_string())
}

/// Build, test and package Enso Engine.
#[derive(Clone, Debug, Parser)]
pub struct Arguments {
    /// build kind (dev/nightly)
    #[clap(long, arg_enum, default_value_t = default_kind(), env = crate::BuildKind::NAME)]
    pub kind:       BuildKind,
    /// path to the local copy of the Enso Engine repository
    #[clap(long, maybe_default_os = crate::arg::default_repo_path(), enso_env())]
    pub repo_path:  PathBuf,
    /// identifier of the release to be targeted (necessary for `upload` and `finish` commands)
    #[clap(long, env = enso_build::env::ReleaseId::NAME)]
    pub release_id: Option<u64>,
    /// whether create bundles with Project Manager and Launcher
    #[clap(long)]
    pub bundle:     Option<bool>,
    /// repository that will be targeted for the release info purposes
    #[clap(long, default_value_t = crate::arg::default_repo_remote(), enso_env())]
    pub repo:       RepoContext,
    #[clap(subcommand)]
    pub command:    WhatToDo,
}

impl Arguments {
    pub fn build_configuration(&self) -> BuildConfigurationResolved {
        let mut config = match self.kind {
            BuildKind::Dev => DEV,
            BuildKind::Nightly => NIGHTLY,
        };

        // Update build configuration with a custom arg overrides.
        if matches!(self.command, WhatToDo::Upload(_)) || self.bundle.contains(&true) {
            config.build_launcher_bundle = true;
            config.build_project_manager_bundle = true;
        }

        BuildConfigurationResolved::new(config)
    }

    pub async fn run_context(&self) -> Result<RunContext> {
        // Get default build configuration for a given build kind.
        let config = self.build_configuration();
        let octocrab = setup_octocrab().await?;
        let enso_root = self.repo_path.clone();
        debug!("Received target location: {}", enso_root.display());
        let enso_root = self.repo_path.absolutize()?.to_path_buf();
        debug!("Absolute target location: {}", enso_root.display());


        let operation = match &self.command {
            WhatToDo::Upload(_) => Operation::Release(self.release_operation()?),
            WhatToDo::Run(Run { command_pieces }) =>
                Operation::Run(RunOperation { command_pieces: command_pieces.clone() }),
            WhatToDo::Build(Build {}) => Operation::Build(BuildOperation {}),
        };
        debug!("Operation to perform: {:?}", operation);

        let target_repo = if let Operation::Release(release_op) = &operation {
            Ok(&release_op.repo)
        } else {
            Err(anyhow!("Missing repository information for release operation."))
        };
        let versions = deduce_versions(&octocrab, self.kind, target_repo, &enso_root).await?;
        versions.publish()?;
        debug!("Target version: {versions:?}.");
        let paths = Paths::new_version(&enso_root, versions.version.clone())?;
        let goodies = GoodieDatabase::new()?;
        let inner = crate::project::Context {
            upload_artifacts: true,
            octocrab,
            cache: Cache::new_default().await?,
        };
        Ok(RunContext { inner, config, paths, goodies, operation })
    }

    pub fn release_operation(&self) -> Result<ReleaseOperation> {
        let command = self.command.clone().try_into()?;
        let repo = self.repo.clone();
        Ok(ReleaseOperation { command, repo })
    }
}
