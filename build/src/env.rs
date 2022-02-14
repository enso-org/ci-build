use crate::prelude::*;

use crate::args::BuildKind;

use ide_ci::env::expect_var;
use octocrab::models::ReleaseId;

pub const RELEASE_ID: &str = "ENSO_RELEASE_ID";

pub const BUILD_KIND: &str = "ENSO_BUILD_KIND";

pub const NIGHTLY_EDITIONS_LIMIT: &str = "ENSO_NIGHTLY_EDITIONS_LIMIT";

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
