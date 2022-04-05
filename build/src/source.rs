use crate::prelude::*;

use crate::project::IsTarget;

use ide_ci::models::config::RepoContext;
use octocrab::models::RunId;
use octocrab::Octocrab;

#[derive(Clone, Debug)]
pub enum ExternalSource {
    OngoingCiRun,
    CiRun(CiRunSource),
    LocalFile(PathBuf),
}

#[derive(Debug)]
pub enum Source<Target: IsTarget> {
    BuildLocally(Target::BuildInput),
    External(ExternalSource),
}

#[derive(Clone, Debug)]
pub struct CiRunSource {
    pub octocrab:      Octocrab,
    pub repository:    RepoContext,
    pub run_id:        RunId,
    pub artifact_name: Option<String>,
}

#[derive(Debug)]
pub struct GetTargetJob<Target: IsTarget> {
    pub source:      Source<Target>,
    pub destination: PathBuf,
}
