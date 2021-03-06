//! This module wraps SBT commands that are provided by the Enso Engine's SBT build scripts.

use crate::prelude::*;

use ide_ci::program::with_cwd::WithCwd;
use ide_ci::programs::Sbt;

pub fn verify_generated_package_task(package: &str, path: impl AsRef<Path>) -> String {
    format!(
        "enso/verifyGeneratedPackage {} {}",
        package,
        path.as_ref().join("THIRD-PARTY").display()
    )
}


pub async fn verify_generated_package(
    sbt: &WithCwd<Sbt>,
    package: &str,
    path: impl AsRef<Path>,
) -> Result {
    sbt.cmd()?.arg(verify_generated_package_task(package, path)).run_ok().await
}
