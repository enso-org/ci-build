use crate::prelude::*;

use heck::ToKebabCase;
use std::collections::BTreeMap;

pub fn is_github_hosted() -> String {
    "startsWith(runner.name, 'GitHub Actions') || startsWith(runner.name, 'Hosted Agent')".into()
}

pub fn setup_conda() -> Step {
    // use crate::actions::workflow::definition::step::CondaChannel;
    Step {
        name: Some("Setup conda (GH runners only)".into()),
        uses: Some("s-weigand/setup-conda@v1.0.5".into()),
        r#if: Some(is_github_hosted()),
        with: Some(step::Argument::SetupConda {
            update_conda:   Some(false),
            conda_channels: Some("anaconda, conda-forge".into()),
        }),
        ..default()
    }
}

pub fn setup_wasm_pack_step() -> Step {
    Step {
        name: Some("Installing wasm-pack".into()),
        uses: Some("jetli/wasm-pack-action@v0.3.0".into()),
        with: Some(step::Argument::Other(BTreeMap::from_iter([(
            "version".into(),
            "v0.10.2".into(),
        )]))),
        r#if: Some(is_github_hosted()),
        ..default()
    }
}

pub fn setup_artifact_api() -> Step {
    let script = [
        r#"core.exportVariable("ACTIONS_RUNTIME_TOKEN", process.env["ACTIONS_RUNTIME_TOKEN"])"#,
        r#"core.exportVariable("ACTIONS_RUNTIME_URL", process.env["ACTIONS_RUNTIME_URL"])"#,
        r#"core.exportVariable("GITHUB_RETENTION_DAYS", process.env["GITHUB_RETENTION_DAYS"])"#,
    ]
    .join("\n");
    Step {
        name: Some("Setup the Artifact API environment".into()),
        uses: Some("actions/github-script@v6".into()),
        with: Some(step::Argument::GitHubScript { script }),
        ..default()
    }
}

pub fn run(os: OS, command_line: impl AsRef<str>) -> Step {
    let bash_step = Step {
        run: Some(format!("./run {}", command_line.as_ref())),
        // r#if: Some("runner.os != 'Windows'".into()),
        shell: Some(Shell::Bash),
        env: once(github_token_env()).collect(),
        ..default()
    };

    let cmd_step = Step {
        run: Some(format!(r".\run.cmd {}", command_line.as_ref())),
        // r#if: Some("runner.os == 'Windows'".into()),
        shell: Some(Shell::Cmd),
        env: once(github_token_env()).collect(),
        ..default()
    };
    if os == OS::Windows {
        cmd_step
    } else {
        bash_step
    }
}

pub fn cancel_workflow_action() -> Step {
    Step {
        name: Some("Cancel Previous Runs".into()),
        uses: Some("styfle/cancel-workflow-action@0.9.1".into()),
        with: Some(step::Argument::Other(BTreeMap::from_iter([(
            "access_token".into(),
            "${{ github.token }}".into(),
        )]))),
        ..default()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobId(String);

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Workflow {
    pub name:        String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub on:          Event,
    pub jobs:        BTreeMap<String, Job>,
}

impl Workflow {
    pub fn add<J: JobArchetype>(&mut self, os: OS) -> String {
        self.add_customized::<J>(os, |_| {})
    }

    pub fn add_customized<J: JobArchetype>(&mut self, os: OS, f: impl FnOnce(&mut Job)) -> String {
        let (key, mut job) = J::entry(os);
        f(&mut job);
        self.jobs.insert(key.clone(), job);
        key
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Push {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    branches:        Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tags:            Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    branches_ignore: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tags_ignore:     Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    paths:           Vec<PathBuf>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    paths_ignore:    Vec<PathBuf>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PullRequest {}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct WorkflowDispatch {}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Event {
    #[serde(skip_serializing_if = "Option::is_none")]
    push:              Option<Push>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pull_request:      Option<PullRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    workflow_dispatch: Option<WorkflowDispatch>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Job {
    pub name:    String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub needs:   Vec<String>,
    pub runs_on: Vec<RunnerLabel>,
    pub steps:   Vec<Step>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Step {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id:    Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name:  Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uses:  Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run:   Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#if:  Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub with:  Option<step::Argument>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub env:   BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell: Option<Shell>,
}

pub fn github_token_env() -> (String, String) {
    ("GITHUB_TOKEN".into(), "${{ secrets.GITHUB_TOKEN }}".into())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Shell {
    /// Command Prompt.
    Cmd,
    Bash,
    /// Power Shell.
    Pwsh,
}

pub mod step {
    use super::*;


    #[derive(Clone, Debug, Serialize, Deserialize)]
    #[serde(rename_all = "kebab-case")]
    #[serde(untagged)]
    pub enum Argument {
        #[serde(rename_all = "kebab-case")]
        Checkout {
            clean: Option<bool>,
        },
        #[serde(rename_all = "kebab-case")]
        SetupConda {
            #[serde(skip_serializing_if = "Option::is_none")]
            update_conda:   Option<bool>,
            #[serde(skip_serializing_if = "Option::is_none")]
            conda_channels: Option<String>, // conda_channels: Vec<CondaChannel>
        },
        #[serde(rename_all = "kebab-case")]
        GitHubScript {
            script: String,
        },
        Other(BTreeMap<String, String>),
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RunnerLabel {
    #[serde(rename = "self-hosted")]
    SelfHosted,
    #[serde(rename = "macOS")]
    MacOS,
    #[serde(rename = "Linux")]
    Linux,
    #[serde(rename = "Windows")]
    Windows,
    #[serde(rename = "engine")]
    Engine,
    #[serde(rename = "macos-latest")]
    MacOSLatest,
    #[serde(rename = "linux-latest")]
    LinuxLatest,
    #[serde(rename = "windows-latest")]
    WindowsLatest,
    #[serde(rename = "X64")]
    X64,
}

pub fn runs_on(os: OS) -> Vec<RunnerLabel> {
    match os {
        OS::Windows => vec![RunnerLabel::SelfHosted, RunnerLabel::Windows, RunnerLabel::Engine],
        OS::Linux => vec![RunnerLabel::SelfHosted, RunnerLabel::Linux, RunnerLabel::Engine],
        OS::MacOS => vec![RunnerLabel::MacOSLatest],
        _ => todo!("Not supported"),
    }
}

pub fn checkout_repo_step() -> Step {
    Step {
        name: Some("Checking out the repository".into()),
        uses: Some("actions/checkout@v3".into()),
        with: Some(step::Argument::Checkout { clean: Some(false) }),
        ..default()
    }
}

pub fn plain_job(os: OS, name: impl AsRef<str>, command_line: impl AsRef<str>) -> Job {
    let checkout_repo_step = checkout_repo_step();
    let run_step = run(os, command_line);
    let list_everything_on_failure = Step {
        name: Some("List files if failed".into()),
        r#if: Some("failure()".into()),
        run: Some("ls -R".into()),
        ..default()
    };

    let name = format!("{} ({})", name.as_ref(), os);
    let steps = vec![
        setup_conda(),
        setup_wasm_pack_step(),
        setup_artifact_api(),
        checkout_repo_step,
        // We don't care about help but this compiles the script as a single step.
        run(os, "--help"),
        run_step,
        list_everything_on_failure,
    ];
    let runs_on = runs_on(os);
    Job { name, runs_on, steps, ..default() }
}

pub trait JobArchetype {
    fn id_key_base() -> String {
        std::any::type_name::<Self>().to_kebab_case()
    }

    fn key(os: OS) -> String {
        format!("{}-{}", Self::id_key_base(), os)
    }

    fn job(os: OS) -> Job;

    fn entry(os: OS) -> (String, Job) {
        (Self::key(os), Self::job(os))
    }
}

pub mod job {
    use super::*;

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
            let omit_in_pr_body =
                "contains(github.event.pull_request.body,'[ci no changelog needed]')";
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
            plain_job(os, "Lint", "lint")
        }
    }

    pub struct NativeTest;
    impl JobArchetype for NativeTest {
        fn job(os: OS) -> Job {
            plain_job(os, "Native GUI tests", "wasm test --no-wasm")
        }
    }

    pub struct WasmTest;
    impl JobArchetype for WasmTest {
        fn job(os: OS) -> Job {
            plain_job(os, "WASM GUI tests", "wasm test --no-native")
        }
    }

    pub struct IntegrationTest;
    impl JobArchetype for IntegrationTest {
        fn job(os: OS) -> Job {
            plain_job(
                os,
                "IDE integration tests",
                "ide integration-test --project-manager-source current-ci-run",
            )
        }
    }

    pub struct BuildWasm;
    impl JobArchetype for BuildWasm {
        fn job(os: OS) -> Job {
            plain_job(os, "Build GUI (WASM)", "wasm build")
        }
    }

    pub struct BuildProjectManager;
    impl JobArchetype for BuildProjectManager {
        fn job(os: OS) -> Job {
            plain_job(os, "Build Project Manager", "project-manager")
        }
    }

    pub struct PackageIde;
    impl JobArchetype for PackageIde {
        fn job(os: OS) -> Job {
            plain_job(
                os,
                "Package IDE",
                "ide build --wasm-source current-ci-run --project-manager-source current-ci-run",
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate() -> Result {
        let on = Event {
            pull_request:      Some(PullRequest {}),
            workflow_dispatch: Some(WorkflowDispatch {}),
            push:              Some(Push {
                branches: vec!["develop".into(), "unstable".into(), "stable".into()],
                ..default()
            }),
        };
        let mut workflow = Workflow { name: "GUI CI".into(), on, ..default() };

        let primary_os = OS::Linux;
        workflow.add::<job::AssertChangelog>(primary_os);
        workflow.add::<job::CancelWorkflow>(primary_os);
        workflow.add::<job::Lint>(primary_os);
        workflow.add::<job::WasmTest>(primary_os);
        workflow.add::<job::NativeTest>(primary_os);
        workflow.add_customized::<job::IntegrationTest>(primary_os, |job| {
            job.needs.push(job::BuildProjectManager::key(primary_os));
        });

        for os in [OS::Windows, OS::Linux, OS::MacOS] {
            let wasm_job = workflow.add::<job::BuildWasm>(os);
            let project_manager_job = workflow.add::<job::BuildProjectManager>(os);
            workflow.add_customized::<job::PackageIde>(os, |job| {
                job.needs.push(wasm_job);
                job.needs.push(project_manager_job);
            });
        }

        let yaml = serde_yaml::to_string(&workflow)?;
        println!("{yaml}");
        let path = r"H:\NBO\enso4\.github\workflows\gui.yml";
        crate::fs::write(path, yaml)?;
        Ok(())
    }
}
