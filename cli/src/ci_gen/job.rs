use crate::ci_gen::runs_on;
use crate::prelude::*;
use ide_ci::actions::workflow::definition::cancel_workflow_action;
use ide_ci::actions::workflow::definition::checkout_repo_step;
use ide_ci::actions::workflow::definition::Job;
use ide_ci::actions::workflow::definition::JobArchetype;
use ide_ci::actions::workflow::definition::RunnerLabel;
use ide_ci::actions::workflow::definition::Step;
use ide_ci::actions::workflow::definition::Strategy;



// pub struct PlainScriptRunJob {
//     name:        String,
//     script_args: String,
// }
//
// impl PlainScriptRunJob {
//     pub fn new(name: impl Into<String>, script_args: impl Into<String>) -> Self {
//         Self { name: name.into(), script_args: script_args.into() }
//     }
// }
//
// impl JobArchetype for PlainScriptRunJob {
//     fn job(os: OS) -> Job {
//         plain_job(&os, "WASM GUI tests", "wasm test --no-native")
//     }
// }

pub trait RunsOn {
    fn strategy(&self) -> Option<Strategy> {
        None
    }
    fn runs_on(&self) -> Vec<RunnerLabel>;
    fn os_name(&self) -> Option<String> {
        None
    }
}

impl RunsOn for OS {
    fn runs_on(&self) -> Vec<RunnerLabel> {
        runs_on(*self)
    }
    fn os_name(&self) -> Option<String> {
        Some(self.to_string())
    }
}

impl RunsOn for Strategy {
    fn strategy(&self) -> Option<Strategy> {
        Some(self.clone())
    }

    fn runs_on(&self) -> Vec<RunnerLabel> {
        vec![RunnerLabel::MatrixOs]
    }
}

pub fn plain_job(
    runs_on_info: &impl RunsOn,
    name: impl AsRef<str>,
    command_line: impl AsRef<str>,
) -> Job {
    let name = if let Some(os_name) = runs_on_info.os_name() {
        format!("{} ({})", name.as_ref(), os_name)
    } else {
        name.as_ref().to_string()
    };
    let steps = crate::ci_gen::setup_script_and_steps(command_line);
    let runs_on = runs_on_info.runs_on();
    let strategy = runs_on_info.strategy();
    Job { name, runs_on, steps, strategy, ..default() }
}

pub struct AssertChangelog;
impl JobArchetype for AssertChangelog {
    fn job(os: OS) -> Job {
        let changed_files = r#"
git fetch
list=`git diff --name-only origin/develop HEAD | tr '\n' ' '`
echo $list
echo "::set-output name=list::'$list'"
"#
        .trim()
        .to_string();

        let changed_files_id = "changed_files";
        let changelog_was_changed =
            format!("contains(steps.{changed_files_id}.outputs.list,'CHANGELOG.md')");
        let omit_in_commit_msg =
            "contains(github.event.head_commit.message,'[ci no changelog needed]')";
        let omit_in_pr_body = "contains(github.event.pull_request.body,'[ci no changelog needed]')";
        let is_dependabot = "github.event.pull_request.user.login == 'dependabot'";

        Job {
                name: "Assert if CHANGELOG.md was updated (on pull request)".into(),
                runs_on: runs_on(os),
                steps: vec![
                    checkout_repo_step(),
                    Step {
                        id: Some(changed_files_id.into()),
                        run: Some(changed_files),
                        ..default()
                    },
                    Step {
                        run: Some(format!("if [[ ${{{{ {changelog_was_changed} || {omit_in_commit_msg} || {omit_in_pr_body} || {is_dependabot} }}}} == false ]]; then exit 1; fi")),
                        r#if: Some("github.base_ref == 'develop' || github.base_ref == 'unstable' || github.base_ref == 'stable'".into()),
                        ..default()
                    }],
                ..default()
            }
    }
}

pub struct CancelWorkflow;
impl JobArchetype for CancelWorkflow {
    fn job(_os: OS) -> Job {
        Job {
            name: "Cancel Previous Runs".into(),
            // It is important that this particular job runs pretty much everywhere (we use x64,
            // as all currently available GH runners have this label). If we limited it only to
            // our self-hosted machines (as we usually do), it'd be enqueued after other jobs
            // and wouldn't be able to cancel them.
            runs_on: vec![RunnerLabel::X64],
            steps: vec![cancel_workflow_action()],
            ..default()
        }
    }
}

pub struct Lint;
impl JobArchetype for Lint {
    fn job(os: OS) -> Job {
        plain_job(&os, "Lint", "lint")
    }
}

pub struct NativeTest;
impl JobArchetype for NativeTest {
    fn job(os: OS) -> Job {
        plain_job(&os, "Native GUI tests", "wasm test --no-wasm")
    }
}

pub struct WasmTest;
impl JobArchetype for WasmTest {
    fn job(os: OS) -> Job {
        plain_job(&os, "WASM GUI tests", "wasm test --no-native")
    }
}

pub struct IntegrationTest;
impl JobArchetype for IntegrationTest {
    fn job(os: OS) -> Job {
        plain_job(
            &os,
            "IDE integration tests",
            "ide integration-test --backend-source current-ci-run",
        )
    }
}

pub struct BuildWasm;
impl JobArchetype for BuildWasm {
    fn job(os: OS) -> Job {
        plain_job(
            &os,
            "Build GUI (WASM)",
            "wasm build --upload-artifacts ${{ runner.os == 'Linux' }}",
        )
    }
}

pub struct BuildBackend;
impl JobArchetype for BuildBackend {
    fn job(os: OS) -> Job {
        plain_job(&os, "Build Backend", "backend get")
    }
}

pub struct UploadBackend;
impl JobArchetype for UploadBackend {
    fn job(os: OS) -> Job {
        plain_job(&os, "Upload Backend", "backend upload")
    }
}

pub struct PackageIde;
impl JobArchetype for PackageIde {
    fn job(os: OS) -> Job {
        plain_job(
            &os,
            "Package IDE",
            "ide build --wasm-source current-ci-run --backend-source current-ci-run",
        )
    }
}
