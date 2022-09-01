use crate::ci_gen::job::expose_os_specific_signing_secret;
use crate::ci_gen::job::plain_job;
use crate::ci_gen::job::plain_job_customized;
use crate::ci_gen::job::RunsOn;
use crate::prelude::*;
use ide_ci::actions::workflow::definition::checkout_repo_step;
use ide_ci::actions::workflow::definition::is_non_windows_runner;
use ide_ci::actions::workflow::definition::is_windows_runner;
use ide_ci::actions::workflow::definition::run;
use ide_ci::actions::workflow::definition::setup_artifact_api;
use ide_ci::actions::workflow::definition::setup_conda;
use ide_ci::actions::workflow::definition::setup_wasm_pack_step;
use ide_ci::actions::workflow::definition::wrap_expression;
use ide_ci::actions::workflow::definition::Concurrency;
use ide_ci::actions::workflow::definition::Event;
use ide_ci::actions::workflow::definition::Job;
use ide_ci::actions::workflow::definition::JobArchetype;
use ide_ci::actions::workflow::definition::PullRequest;
use ide_ci::actions::workflow::definition::Push;
use ide_ci::actions::workflow::definition::RunnerLabel;
use ide_ci::actions::workflow::definition::Schedule;
use ide_ci::actions::workflow::definition::Step;
use ide_ci::actions::workflow::definition::Workflow;
use ide_ci::actions::workflow::definition::WorkflowDispatch;
use ide_ci::actions::workflow::definition::WorkflowDispatchInput;
use ide_ci::actions::workflow::definition::WorkflowDispatchInputType;
use std::convert::identity;

pub mod job;
pub mod step;

pub struct DeluxeRunner;
pub struct BenchmarkRunner;

pub const PRIMARY_OS: OS = OS::Linux;

pub const TARGETED_SYSTEMS: [OS; 3] = [OS::Windows, OS::Linux, OS::MacOS];

pub const DEFAULT_BRANCH_NAME: &str = "develop";

/// Name of the GitHub Actions secret that stores path to the Windows code signing certificate
/// within the runner.
pub const SECRET_WINDOWS_CERT_PATH: &str = "MICROSOFT_CODE_SIGNING_CERT";

/// Name of the GitHub Actions secret that stores password to the Windows code signing certificate.
pub const SECRET_WINDOWS_CERT_PASSWORD: &str = "MICROSOFT_CODE_SIGNING_CERT_PASSWORD";

pub const ECR_PUSH_RUNTIME_SECRET_ACCESS_KEY: &str = "ECR_PUSH_RUNTIME_SECRET_ACCESS_KEY";

pub const ECR_PUSH_RUNTIME_ACCESS_KEY_ID: &str = "ECR_PUSH_RUNTIME_ACCESS_KEY_ID";

impl RunsOn for DeluxeRunner {
    fn runs_on(&self) -> Vec<RunnerLabel> {
        vec![RunnerLabel::MwuDeluxe]
    }
    fn os_name(&self) -> Option<String> {
        None
    }
}

impl RunsOn for BenchmarkRunner {
    fn runs_on(&self) -> Vec<RunnerLabel> {
        vec![RunnerLabel::Benchmark]
    }
    fn os_name(&self) -> Option<String> {
        None
    }
}

pub fn on_develop_push() -> Push {
    Push { branches: vec![DEFAULT_BRANCH_NAME.to_string()], ..default() }
}

pub fn runs_on(os: OS) -> Vec<RunnerLabel> {
    match os {
        OS::Windows => vec![RunnerLabel::SelfHosted, RunnerLabel::Windows, RunnerLabel::Engine],
        OS::Linux => vec![RunnerLabel::SelfHosted, RunnerLabel::Linux, RunnerLabel::Engine],
        OS::MacOS => vec![RunnerLabel::MacOSLatest],
        _ => todo!("Not supported"),
    }
}

pub fn setup_script_steps() -> Vec<Step> {
    let mut ret = vec![setup_conda(), setup_wasm_pack_step(), setup_artifact_api()];
    ret.extend(checkout_repo_step());
    ret.push(run("--help").with_name("Build Script Setup"));
    ret
}

pub fn list_everything_on_failure() -> impl IntoIterator<Item = Step> {
    let win = Step {
        name: Some("List files if failed (Windows)".into()),
        r#if: Some(format!("failure() && {}", is_windows_runner())),
        run: Some("Get-ChildItem -Force -Recurse".into()),
        ..default()
    };

    let non_win = Step {
        name: Some("List files if failed (non-Windows)".into()),
        r#if: Some(format!("failure() && {}", is_non_windows_runner())),
        run: Some("ls -lAR".into()),
        ..default()
    };

    [win, non_win]
}


/// The `f` is applied to the step that does an actual script invocation.
pub fn setup_customized_script_steps(
    command_line: impl AsRef<str>,
    f: impl FnOnce(Step) -> Step,
) -> Vec<Step> {
    let mut steps = setup_script_steps();
    let run_step = f(run(command_line));
    steps.push(run_step);
    steps.extend(list_everything_on_failure());
    steps
}

pub fn setup_script_and_steps(command_line: impl AsRef<str>) -> Vec<Step> {
    setup_customized_script_steps(command_line, identity)
}

pub struct DraftRelease;
impl JobArchetype for DraftRelease {
    fn job(os: OS) -> Job {
        let name = "Create release draft".into();

        let prepare_step = run("release create-draft").with_id(Self::PREPARE_STEP_ID);

        let mut steps = setup_script_steps();
        steps.push(prepare_step);

        let mut ret = Job { name, runs_on: runs_on(os), steps, ..default() };
        Self::expose_outputs(&mut ret);
        ret
    }

    fn outputs() -> BTreeMap<String, Vec<String>> {
        let mut ret = BTreeMap::new();
        ret.insert(Self::PREPARE_STEP_ID.into(), vec![
            "ENSO_VERSION".into(),
            "ENSO_RELEASE_ID".into(),
        ]);
        ret
    }
}

impl DraftRelease {
    pub const PREPARE_STEP_ID: &'static str = "prepare";
}

pub struct PublishRelease;
impl JobArchetype for PublishRelease {
    fn job(os: OS) -> Job {
        let mut ret = plain_job(&os, "Publish release", "release publish");
        ret.expose_secret_as("ARTEFACT_S3_ACCESS_KEY_ID", "AWS_ACCESS_KEY_ID");
        ret.expose_secret_as("ARTEFACT_S3_SECRET_ACCESS_KEY ", "AWS_SECRET_ACCESS_KEY");
        ret.env("AWS_REGION", "us-west-1");
        ret
    }
}

pub struct UploadIde;
impl JobArchetype for UploadIde {
    fn job(os: OS) -> Job {
        plain_job_customized(&os, "Build IDE", "ide upload --wasm-source current-ci-run --backend-source release --backend-release ${{env.ENSO_RELEASE_ID}}", |step| 
            expose_os_specific_signing_secret(os, step)
        )
    }
}

pub fn nightly() -> Result<Workflow> {
    let on = Event {
        workflow_dispatch: Some(default()),
        // 5am (UTC) from Tuesday to Saturday (i.e. after every workday)
        schedule: vec![Schedule::new("0 5 * * 2-6")?],
        ..default()
    };

    let linux_only = OS::Linux;

    let concurrency_group = "release";
    let mut workflow = Workflow {
        on,
        name: "Nightly Release".into(),
        concurrency: Some(Concurrency::new(concurrency_group)),
        ..default()
    };

    let prepare_job_id = workflow.add::<DraftRelease>(linux_only);
    let build_wasm_job_id = workflow.add::<job::BuildWasm>(linux_only);
    let mut packaging_job_ids = vec![];

    // Assumed, because Linux is necessary to deploy ECR runtime image.
    assert!(TARGETED_SYSTEMS.contains(&OS::Linux));

    for os in TARGETED_SYSTEMS {
        let backend_job_id = workflow.add_dependent::<job::UploadBackend>(os, [&prepare_job_id]);
        let build_ide_job_id = workflow.add_dependent::<UploadIde>(os, [
            &prepare_job_id,
            &backend_job_id,
            &build_wasm_job_id,
        ]);
        packaging_job_ids.push(build_ide_job_id);

        if os == OS::Linux {
            let upload_runtime_job_id = workflow
                .add_dependent::<job::UploadRuntimeToEcr>(os, [&prepare_job_id, &backend_job_id]);
            packaging_job_ids.push(upload_runtime_job_id);
        }
    }

    let publish_deps = {
        packaging_job_ids.push(prepare_job_id);
        packaging_job_ids
    };

    let _publish_job_id = workflow.add_dependent::<PublishRelease>(linux_only, publish_deps);
    let global_env = [("ENSO_BUILD_KIND", "nightly"), ("RUST_BACKTRACE", "full")];
    for (var_name, value) in global_env {
        workflow.env(var_name, value);
    }
    Ok(workflow)
}

pub fn typical_check_triggers() -> Event {
    Event {
        pull_request: Some(PullRequest {}),
        workflow_dispatch: Some(default()),
        push: Some(on_develop_push()),
        ..default()
    }
}

pub fn gui() -> Result<Workflow> {
    let on = typical_check_triggers();
    let mut workflow = Workflow { name: "GUI CI".into(), on, ..default() };
    workflow.add::<job::AssertChangelog>(PRIMARY_OS);
    workflow.add::<job::CancelWorkflow>(PRIMARY_OS);
    workflow.add::<job::Lint>(PRIMARY_OS);
    workflow.add::<job::WasmTest>(PRIMARY_OS);
    workflow.add::<job::NativeTest>(PRIMARY_OS);

    // FIXME: Integration tests are currently always failing.
    //        The should be reinstated when fixed.
    // workflow.add_customized::<job::IntegrationTest>(PRIMARY_OS, |job| {
    //     job.needs.insert(job::BuildBackend::key(PRIMARY_OS));
    // });

    // Because WASM upload happens only for the Linux build, all other platforms needs to depend on
    // it.
    let wasm_job_linux = workflow.add::<job::BuildWasm>(OS::Linux);
    for os in TARGETED_SYSTEMS {
        if os != OS::Linux {
            // Linux was already added above.
            let _wasm_job = workflow.add::<job::BuildWasm>(os);
        }
        let project_manager_job = workflow.add::<job::BuildBackend>(os);
        workflow.add_customized::<job::PackageIde>(os, |job| {
            job.needs.insert(wasm_job_linux.clone());
            job.needs.insert(project_manager_job);
        });
    }
    Ok(workflow)
}

pub fn backend() -> Result<Workflow> {
    let on = typical_check_triggers();
    let mut workflow = Workflow { name: "Engine CI".into(), on, ..default() };
    workflow.add::<job::CancelWorkflow>(PRIMARY_OS);
    for os in TARGETED_SYSTEMS {
        workflow.add::<job::CiCheckBackend>(os);
    }
    Ok(workflow)
}

pub fn benchmark() -> Result<Workflow> {
    let just_check_input_name = "just-check";
    let just_check_input = WorkflowDispatchInput {
        r#type: WorkflowDispatchInputType::Boolean{default: Some(false)},
        ..WorkflowDispatchInput::new("If set, benchmarks will be only checked to run correctly, not to measure actual performance.", true)
    };
    let on = Event {
        push: Some(on_develop_push()),
        workflow_dispatch: Some(
            WorkflowDispatch::default().with_input(just_check_input_name, just_check_input),
        ),
        schedule: vec![Schedule::new("0 5 * * 2-6")?],
        ..default()
    };
    let mut workflow = Workflow { name: "Benchmark Engine".into(), on, ..default() };
    workflow
        .env("ENSO_BUILD_MINIMAL_RUN", wrap_expression(format!("inputs.{just_check_input_name}")));

    let benchmark_job =
        plain_job(&BenchmarkRunner, "Benchmark Engine", "backend benchmark runtime");
    workflow.add_job(benchmark_job);
    Ok(workflow)
}


pub fn generate(repo_root: &enso_build::paths::generated::RepoRootGithubWorkflows) -> Result {
    repo_root.nightly_yml.write_as_yaml(&nightly()?)?;
    repo_root.scala_new_yml.write_as_yaml(&backend()?)?;
    repo_root.gui_yml.write_as_yaml(&gui()?)?;
    repo_root.benchmark_yml.write_as_yaml(&benchmark()?)?;
    Ok(())
}
