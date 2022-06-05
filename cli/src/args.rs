use enso_build::prelude::*;

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
use enso_build::BuildKind;

use enso_build::paths::Paths;
use ide_ci::env::Variable;
use ide_ci::goodie::GoodieDatabase;
use ide_ci::models::config::RepoContext;

#[derive(FromArgs, Clone, PartialEq, Debug, strum::Display)]
#[argh(subcommand)]
pub enum WhatToDo {
    Build(Build),
    // Three release-related commands below.
    Create(Create),
    Upload(Upload),
    Publish(Publish),
    // Utilities
    Run(Run),
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

impl WhatToDo {
    pub fn is_release_command(&self) -> bool {
        use WhatToDo::*;
        // Not using matches! to force manual check when extending enum.
        match self {
            Create(_) => true,
            Upload(_) => true,
            Publish(_) => true,
            Build(_) | Run(_) => false,
        }
    }
}

/// Just build the packages.
#[derive(FromArgs, Clone, PartialEq, Debug)]
#[argh(subcommand, name = "build")]
pub struct Build {}

/// Create a new draft release on GitHub and emit relevant information to the CI environment.
#[derive(FromArgs, Clone, PartialEq, Debug)]
#[argh(subcommand, name = "create-release")]
pub struct Create {}

/// Build all the release packages and bundles and upload them to GitHub release. Must run with
/// environment adjusted by the `prepare` command.
#[derive(FromArgs, Clone, PartialEq, Debug)]
#[argh(subcommand, name = "upload-asset")]
pub struct Upload {}

/// Publish the release.  Must run with environment adjusted by the `prepare` command. Typically
/// called once after platform-specific `upload` commands are done.
#[derive(FromArgs, Clone, PartialEq, Debug)]
#[argh(subcommand, name = "publish-release")]
pub struct Publish {}

/// Run an arbitrary command with the build environment set (like `PATH`).
#[derive(FromArgs, Clone, PartialEq, Debug)]
#[argh(subcommand, name = "run")]
pub struct Run {
    #[argh(positional)]
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

/// Build, test and packave Enso Engine.
#[derive(Clone, Debug, FromArgs)]
pub struct Args {
    /// build kind (dev/nightly)
    #[argh(option, default = "default_kind()")]
    pub kind:       BuildKind,
    /// path to the local copy of the Enso Engine repository
    #[argh(positional)]
    pub target:     PathBuf,
    /// identifier of the release to be targeted (necessary for `upload` and `finish` commands)
    #[argh(option)]
    pub release_id: Option<u64>,
    /// whether create bundles with Project Manager and Launcher
    #[argh(option)]
    pub bundle:     Option<bool>,
    /// repository that will be targeted for the release info purposes
    #[argh(option, from_str_fn(parse_repo_context), default = "default_repo()")]
    pub repo:       Option<RepoContext>,
    #[argh(subcommand)]
    pub command:    WhatToDo,
    /* #[argh(subcommand)]
     * pub task:       Vec<Task>, */
}

impl Args {
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
        let enso_root = self.target.clone();
        debug!("Received target location: {}", enso_root.display());
        let enso_root = self.target.absolutize()?.to_path_buf();
        debug!("Absolute target location: {}", enso_root.display());


        let operation = match &self.command {
            WhatToDo::Create(_) | WhatToDo::Upload(_) | WhatToDo::Publish(_) =>
                Operation::Release(self.release_operation()?),
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
        Ok(RunContext { config, octocrab, paths, goodies, operation })
    }

    pub fn release_operation(&self) -> Result<ReleaseOperation> {
        let command = self.command.clone().try_into()?;
        let repo = match self.repo.clone() {
            Some(repo) => repo,
            None => ide_ci::actions::env::GITHUB_REPOSITORY.get()?,
        };

        Ok(ReleaseOperation { command, repo })
    }
}
