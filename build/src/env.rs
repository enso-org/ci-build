use crate::prelude::*;

use ide_ci::env::Variable;
use ide_ci::programs::docker::ContainerId;

pub struct ReleaseId;
impl Variable for ReleaseId {
    const NAME: &'static str = "ENSO_RELEASE_ID";
    type Value = octocrab::models::ReleaseId;
}

pub struct RunnerContainerName;
impl Variable for RunnerContainerName {
    const NAME: &'static str = "ENSO_RUNNER_CONTAINER_NAME";
    type Value = ContainerId;
}

pub struct NightlyEditionsLimit;
impl Variable for NightlyEditionsLimit {
    const NAME: &'static str = "ENSO_NIGHTLY_EDITIONS_LIMIT";
    type Value = usize;
}

pub struct BuildKind;
impl Variable for BuildKind {
    const NAME: &'static str = "ENSO_BUILD_KIND";
    type Value = crate::args::BuildKind;
}
