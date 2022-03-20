//! Environment variables used by the engine's SBT-based build system.

use crate::prelude::*;

use ide_ci::env::Variable;

pub struct CiTestTimeFactor;
impl Variable for CiTestTimeFactor {
    const NAME: &'static str = "CI_TEST_TIMEFACTOR";
    type Value = usize;
}

pub struct CiFlakyTestEnable;
impl Variable for CiFlakyTestEnable {
    const NAME: &'static str = "CI_TEST_FLAKY_ENABLE";
    type Value = bool;
}
