use crate::prelude::*;
use derivative::Derivative;

use crate::project::IsTarget;
use crate::project::IsWatchable;

use ide_ci::models::config::RepoContext;
use octocrab::models::AssetId;
use octocrab::models::RunId;

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
    #[derivative(Debug(format_with = "std::fmt::Display::fmt"))]
    pub repository:    RepoContext,
    #[derivative(Debug(format_with = "std::fmt::Display::fmt"))]
    pub run_id:        RunId,
    pub artifact_name: String,
}

#[derive(Clone, Derivative)]
#[derivative(Debug)]
pub struct ReleaseSource {
    #[derivative(Debug(format_with = "std::fmt::Display::fmt"))]
    pub repository: RepoContext,
    #[derivative(Debug(format_with = "std::fmt::Display::fmt"))]
    pub asset_id:   AssetId,
}

#[derive(Clone, Debug, derive_more::Deref, derive_more::DerefMut)]
pub struct WithDestination<T> {
    #[deref]
    #[deref_mut]
    pub inner:       T,
    pub destination: PathBuf,
}

impl<T: IsTarget> WithDestination<Source<T>> {
    pub fn to_external(&self) -> Option<FetchTargetJob> {
        match &self.inner {
            Source::BuildLocally(_) => None,
            Source::External(external) => Some(WithDestination {
                inner:       external.clone(),
                destination: self.destination.clone(),
            }),
        }
    }
}

impl<T> WithDestination<T> {
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> WithDestination<U> {
        WithDestination { inner: f(self.inner), destination: self.destination }
    }
}

pub type GetTargetJob<Target> = WithDestination<Source<Target>>;
pub type FetchTargetJob = WithDestination<ExternalSource>;
pub type BuildTargetJob<Target> = WithDestination<<Target as IsTarget>::BuildInput>;

#[derive(Debug)]
pub struct WatchTargetJob<Target: IsWatchable> {
    pub build:       BuildTargetJob<Target>,
    pub watch_input: Target::WatchInput,
}

#[derive(Debug)]
pub enum FetchOrWatch<Target: IsWatchable> {
    Fetch(FetchTargetJob),
    Watch(WatchTargetJob<Target>),
}
