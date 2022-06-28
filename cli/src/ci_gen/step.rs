use crate::prelude::*;

use enso_build::paths;
use ide_ci::actions::workflow::definition::wrap_expression;
use ide_ci::actions::workflow::definition::Step;

pub fn test_reporter() -> Step {
    Step {
        name: Some("Stdlib test report".into()),
        uses: Some("dorny/test-reporter@v1".into()),
        r#if: Some("success() || failure()".into()),
        ..default()
    }
    .with_custom_argument("reporter", "java-junit")
    .with_custom_argument("path", wrap_expression(format!("env.{}", paths::ENSO_TEST_JUNIT_DIR)))
}
