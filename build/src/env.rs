use crate::prelude::*;
use ide_ci::env::expect_var;
use octocrab::models::ReleaseId;

pub const ENSO_RELEASE_ID: &str = "ENSO_RELEASE_ID";

pub fn release_id() -> Result<ReleaseId> {
    Ok(expect_var(ENSO_RELEASE_ID)?.parse::<u64>()?.into())
}

pub fn emit_release_id(id: ReleaseId) {
    ide_ci::actions::workflow::set_output("ENSO_RELEASE_ID", id)
}
