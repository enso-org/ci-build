use crate::prelude::*;

use crate::args::BuildKind;

use ide_ci::env::expect_var;
use ide_ci::programs::docker::ContainerId;
use octocrab::models::ReleaseId;

pub const RELEASE_ID: &str = "ENSO_RELEASE_ID";

pub const BUILD_KIND: &str = "ENSO_BUILD_KIND";

pub const NIGHTLY_EDITIONS_LIMIT: &str = "ENSO_NIGHTLY_EDITIONS_LIMIT";

pub const RUNNER_CONTAINER_NAME: &str = "ENSO_RUNNER_CONTAINER_NAME";

pub fn release_id() -> Result<ReleaseId> {
    u64::parse_into(expect_var(RELEASE_ID)?)
}

pub fn emit_release_id(id: ReleaseId) {
    ide_ci::actions::workflow::set_output(RELEASE_ID, id)
}

pub fn build_kind() -> Result<BuildKind> {
    expect_var(BUILD_KIND)?.parse2()
}

pub fn nightly_editions_limit() -> Result<usize> {
    expect_var(NIGHTLY_EDITIONS_LIMIT)?.parse2()
}

pub fn runner_container_name() -> Result<ContainerId> {
    expect_var(RUNNER_CONTAINER_NAME)?.parse2()
}
