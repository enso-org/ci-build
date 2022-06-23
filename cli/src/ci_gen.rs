use crate::ci_gen::job::plain_job;
use crate::ci_gen::job::RunsOn;
use crate::prelude::*;
use ide_ci::actions::workflow::definition::checkout_repo_step;
use ide_ci::actions::workflow::definition::run;
use ide_ci::actions::workflow::definition::setup_artifact_api;
use ide_ci::actions::workflow::definition::setup_conda;
use ide_ci::actions::workflow::definition::setup_wasm_pack_step;
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

pub mod job;

pub struct DeluxeRunner;

impl RunsOn for DeluxeRunner {
    fn runs_on(&self) -> Vec<RunnerLabel> {
        vec![RunnerLabel::MwuDeluxe]
    }

    fn os_name(&self) -> Option<String> {
        None
    }
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
    let mut ret =
        vec![setup_conda(), setup_wasm_pack_step(), setup_artifact_api(), checkout_repo_step()];
    ret.push(run("--help"));
    ret
}

pub fn setup_script_and_steps(command_line: impl AsRef<str>) -> Vec<Step> {
    let list_everything_on_failure = Step {
        name: Some("List files if failed".into()),
        r#if: Some("failure()".into()),
        run: Some("ls -R".into()),
        ..default()
    };
    let mut steps = setup_script_steps();
    steps.push(run(command_line));
    steps.push(list_everything_on_failure);
    steps
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
        plain_job(&os, "Build IDE", "ide upload --wasm-source current-ci-run --backend-source release --backend-release ${{env.ENSO_RELEASE_ID}}")
    }
}

pub fn nightly() -> Result<Workflow> {
    let on = Event {
        workflow_dispatch: Some(WorkflowDispatch {}),
        // 5am (UTC) from Tuesday to Saturday (i.e. after every workday)
        schedule: vec![Schedule::new("0 5 * * 2-6")?],
        ..default()
    };

    let targeted_platforms = [OS::Windows, OS::Linux, OS::MacOS];
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
    for os in targeted_platforms {
        let backend_job_id = workflow.add_dependent::<job::UploadBackend>(os, [&prepare_job_id]);
        let build_ide_job_id = workflow.add_dependent::<UploadIde>(os, [
            &prepare_job_id,
            &backend_job_id,
            &build_wasm_job_id,
        ]);
        packaging_job_ids.push(build_ide_job_id);
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
    workflow.env("ENSO_BUILD_SKIP_VERSION_CHECK", "true");
    Ok(workflow)
}

pub fn gui() -> Result<Workflow> {
    let on = Event {
        pull_request: Some(PullRequest {}),
        workflow_dispatch: Some(WorkflowDispatch {}),
        push: Some(Push {
            branches: vec!["develop".into(), "unstable".into(), "stable".into()],
            ..default()
        }),
        ..default()
    };
    let mut workflow = Workflow { name: "GUI CI".into(), on, ..default() };
    workflow.env("ENSO_BUILD_SKIP_VERSION_CHECK", "true");
    let primary_os = OS::Linux;
    workflow.add::<job::AssertChangelog>(primary_os);
    workflow.add::<job::CancelWorkflow>(primary_os);
    workflow.add::<job::Lint>(primary_os);
    workflow.add::<job::WasmTest>(primary_os);
    workflow.add::<job::NativeTest>(primary_os);
    workflow.add_customized::<job::IntegrationTest>(primary_os, |job| {
        job.needs.insert(job::BuildBackend::key(primary_os));
    });

    for os in [OS::Windows, OS::Linux, OS::MacOS] {
        let wasm_job = workflow.add::<job::BuildWasm>(os);
        let project_manager_job = workflow.add::<job::BuildBackend>(os);
        workflow.add_customized::<job::PackageIde>(os, |job| {
            job.needs.insert(wasm_job);
            job.needs.insert(project_manager_job);
        });
    }
    Ok(workflow)
}

pub fn generate(repo_root: &enso_build::paths::generated::RepoRootGithubWorkflows) -> Result {
    repo_root.nightly_yml.write_as_yaml(&nightly()?)?;
    repo_root.gui_yml.write_as_yaml(&gui()?)?;
    Ok(())
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use enso_build::paths::generated::RepoRootGithubWorkflows;
//     use enso_build::setup_octocrab;
//     use futures_util::future::join_all;
//     use ide_ci::future::try_join_all;
//     use ide_ci::future::AsyncPolicy;
//     use ide_ci::log::setup_logging;
//     use octocrab::models::workflows::Run;
//     use octocrab::models::RunId;
//
//     #[test]
//     fn generate_test() -> Result {
//         let repo_path = r"H:/NBO/enso-staging";
//         generate(&RepoRootGithubWorkflows::new(repo_path))?;
//         Ok(())
//     }
//
//     async fn cancel_workflow_run(octo: &Octocrab, run_id: RunId) -> Result<serde_json::Value> {
//         debug!("Will cancel {}", run_id);
//         let owner = "enso-org";
//         let repo = "enso";
//         octo.post::<(), serde_json::Value>(
//             format!("/repos/{owner}/{repo}/actions/runs/{run_id}/cancel"),
//             Option::<&()>::None,
//         )
//         .void_ok()
//         .anyhow_err()
//         .await
//         .context(format!("Cancelling run {run_id}."))
//     }
//
//     async fn delete_workflow_run(octo: &Octocrab, run_id: RunId) -> Result {
//         debug!("Will delete {}", run_id);
//         let owner = "enso-org";
//         let repo = "enso";
//         octo.delete::<serde_json::Value, _, _>(
//             format!("/repos/{owner}/{repo}/actions/runs/{run_id}"),
//             Option::<&()>::None,
//         )
//         .void_ok()
//         .await
//         .context(format!("Deleting run {run_id}."))
//     }
//
//     #[tokio::test]
//     async fn remove_extra_runs2() -> Result {
//         setup_logging()?;
//         let owner = "enso-org";
//         let repo = "enso";
//         let octo: &'static Octocrab = Box::leak(Box::new(setup_octocrab().await?));
//         // delete_workflow_run(octo, 2541547617.into()).await?;
//         delete_workflow_run(octo, 2541547617.into()).await?;
//         Ok(())
//     }
//
//     #[tokio::test]
//     async fn remove_extra_runs() -> Result {
//         setup_logging()?;
//         let owner = "enso-org";
//         let repo = "enso";
//         let octo: &'static Octocrab = Box::leak(Box::new(setup_octocrab().await?));
//         let mut runs_page =
//             octo.workflows("enso-org", "enso").list_all_runs().per_page(100).send().await?;
//         // let all_runs = octo.all_pages(runs_page).await?;
//         let path = PathBuf::from("runs.yaml");
//         path.write_as_yaml(&runs_page.items)?;
//
//         let file = ide_ci::fs::create("runs4.yaml")?;
//
//         let mut tasks = vec![];
//
//         let mut i = 0;
//         // while let Some(page_ok) = octo.get_page::<Run>(&runs_page.next).await? {
//         //     runs_page = page_ok;
//         serde_yaml::to_writer(&file, &runs_page.items)?;
//         if let Some(first) = runs_page.items.first() {
//             debug!("{}", first.created_at);
//         }
//         let jobs = runs_page.items.into_iter().filter(|run| run.head_commit.message == "Merge
// branch 'develop' into wip/michaelmauderer/Component_List_Panel_View_#180892146").map(|run| {
//                 async move {
//                     if run.status == "queued" {
//                         cancel_workflow_run(octo, run.id).await?;
//                         debug!("Done with {}", run.id);
//                     }
//                     // delete_workflow_run(octo, run.id).await
//                     Result::Ok(())
//                 }
//             }).collect_vec();
//
//         tasks.push(tokio::spawn(async move {
//             let result = join_all(jobs).await;
//             debug!("{:?}", result);
//             ()
//         }));
//
//         // i += 10;
//         // debug!("#{i}");
//         // if i >= 1 {
//         //     break;
//         // }
//         // }
//
//         try_join_all(tasks, AsyncPolicy::Sequential).await?;
//
//
//
//         Ok(())
//     }
// }
