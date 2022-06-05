use crate::prelude::*;
use derivative::Derivative;

use crate::project::IsTarget;

use ide_ci::models::config::RepoContext;
use octocrab::models::AssetId;
use octocrab::models::RunId;
use octocrab::Octocrab;

#[derive(Clone, Derivative)]
#[derivative(Debug)]
pub enum ExternalSource {
    #[derivative(Debug = "transparent")]
    OngoingCiRun(OngoingCiRunSource),
    #[derivative(Debug = "transparent")]
    CiRun(CiRunSource),
    #[derivative(Debug = "transparent")]
    LocalFile(PathBuf),
    #[derivative(Debug = "transparent")]
    Release(ReleaseSource),
}

#[derive(Derivative)]
#[derivative(Debug)]
pub enum Source<Target: IsTarget> {
    #[derivative(Debug = "transparent")]
    BuildLocally(Target::BuildInput),
    #[derivative(Debug = "transparent")]
    External(ExternalSource),
}

#[derive(Clone, Debug)]
pub struct OngoingCiRunSource {
    pub artifact_name: String,
}

#[derive(Clone, Derivative)]
#[derivative(Debug)]
pub struct CiRunSource {
    #[derivative(Debug = "ignore")]
    pub octocrab:      Octocrab,
    #[derivative(Debug(format_with = "std::fmt::Display::fmt"))]
    pub repository:    RepoContext,
    #[derivative(Debug(format_with = "std::fmt::Display::fmt"))]
    pub run_id:        RunId,
    pub artifact_name: String,
}

#[derive(Clone, Derivative)]
#[derivative(Debug)]
pub struct ReleaseSource {
    #[derivative(Debug = "ignore")]
    pub octocrab:   Octocrab,
    #[derivative(Debug(format_with = "std::fmt::Display::fmt"))]
    pub repository: RepoContext,
    #[derivative(Debug(format_with = "std::fmt::Display::fmt"))]
    pub asset_id:   AssetId,
}

#[derive(Debug)]
pub struct GetTargetJob<Target: IsTarget> {
    pub source:      Source<Target>,
    pub destination: PathBuf,
}
