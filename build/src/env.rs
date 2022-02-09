use crate::args::BuildKind;
use crate::prelude::*;
use ide_ci::env::expect_var;
use octocrab::models::ReleaseId;

pub const ENSO_RELEASE_ID: &str = "ENSO_RELEASE_ID";
pub const ENSO_BUILD_KIND: &str = "ENSO_BUILD_KIND";

pub fn release_id() -> Result<ReleaseId> {
    u64::parse_into(expect_var(ENSO_RELEASE_ID)?)
}

pub fn emit_release_id(id: ReleaseId) {
    ide_ci::actions::workflow::set_output(ENSO_RELEASE_ID, id)
}

pub fn build_kind() -> Result<BuildKind> {
    Ok(expect_var(ENSO_BUILD_KIND)?.parse()?)
}
