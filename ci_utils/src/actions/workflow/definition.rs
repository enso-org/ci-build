use crate::prelude::*;

use heck::ToKebabCase;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

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

pub fn shell_os(os: OS, command_line: impl Into<String>) -> Step {
    Step {
        run: Some(command_line.into()),
        env: once(github_token_env()).collect(),
        r#if: Some(format!("runner.os {} 'Windows'", if os == OS::Windows { "==" } else { "!=" })),
        shell: Some(if os == OS::Windows { Shell::Pwsh } else { Shell::Bash }),
        ..default()
    }
}

pub fn shell(command_line: impl AsRef<str>) -> Vec<Step> {
    vec![shell_os(OS::Windows, command_line.as_ref()), shell_os(OS::Linux, command_line.as_ref())]
}

pub fn run(run_args: impl AsRef<str>) -> Vec<Step> {
    shell(format!("./run {}", run_args.as_ref()))
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
    pub env:         BTreeMap<String, String>,
}

impl Workflow {
    pub fn expose_outputs(&self, source_job_id: impl AsRef<str>, consumer_job: &mut Job) {
        let source_job = self.jobs.get(source_job_id.as_ref()).unwrap();
        consumer_job.use_job_outputs(source_job_id.as_ref(), source_job);
    }
}

impl Workflow {
    pub fn add_job(&mut self, job: Job) -> String {
        let key = job.name.to_kebab_case();
        self.jobs.insert(key.clone(), job);
        key
    }

    pub fn add<J: JobArchetype>(&mut self, os: OS) -> String {
        self.add_customized::<J>(os, |_| {})
    }

    pub fn add_customized<J: JobArchetype>(&mut self, os: OS, f: impl FnOnce(&mut Job)) -> String {
        let (key, mut job) = J::entry(os);
        f(&mut job);
        self.jobs.insert(key.clone(), job);
        key
    }

    pub fn env(&mut self, var_name: impl Into<String>, var_value: impl Into<String>) {
        self.env.insert(var_name.into(), var_value.into());
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
    pub name:     String,
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub needs:    BTreeSet<String>,
    pub runs_on:  Vec<RunnerLabel>,
    pub steps:    Vec<Step>,
    pub outputs:  BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy: Option<Strategy>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub env:      BTreeMap<String, String>,
}

impl Job {
    pub fn expose_output(&mut self, step_id: impl AsRef<str>, output_name: impl Into<String>) {
        let step = step_id.as_ref();
        let output = output_name.into();
        let value = format!("${{{{ steps.{step}.outputs.{output} }}}}");
        self.outputs.insert(output, value);
    }

    pub fn env(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.env.insert(name.into(), value.into());
    }

    pub fn expose_secret_as(&mut self, secret: impl AsRef<str>, given_name: impl Into<String>) {
        self.env(given_name, format!("${{{{ secrets.{} }}}}", secret.as_ref()));
    }

    pub fn use_job_outputs(&mut self, job_id: impl Into<String>, job: &Job) {
        let job_id = job_id.into();
        for (output_name, _) in &job.outputs {
            let reference = format!("${{{{needs.{}.outputs.{}}}}}", job_id, output_name);
            self.env.insert(output_name.into(), reference);
        }
        self.needs(job_id);
    }

    pub fn needs(&mut self, job_id: impl Into<String>) {
        self.needs.insert(job_id.into());
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Strategy {
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub matrix:    BTreeMap<String, serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fail_fast: Option<bool>,
}

impl Strategy {
    pub fn new_os(labels: impl Serialize) -> Strategy {
        let oses = serde_json::to_value(labels).unwrap();
        Strategy {
            fail_fast: Some(false),
            matrix:    [("os".to_string(), oses)].into_iter().collect(),
        }
    }
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
    #[serde(rename = "mwu-deluxe")]
    MwuDeluxe,
    #[serde(rename = "${{ matrix.os }}")]
    MatrixOs,
}

pub fn runs_on(os: OS) -> Vec<RunnerLabel> {
    match os {
        OS::Windows => vec![RunnerLabel::SelfHosted, RunnerLabel::Windows, RunnerLabel::Engine],
        OS::Linux => vec![RunnerLabel::SelfHosted, RunnerLabel::Linux, RunnerLabel::MwuDeluxe],
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

pub fn setup_script_steps() -> Vec<Step> {
    let mut ret =
        vec![setup_conda(), setup_wasm_pack_step(), setup_artifact_api(), checkout_repo_step()];
    ret.extend(run("--help"));
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
    steps.extend(run(command_line));
    steps.push(list_everything_on_failure);
    steps
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
    let steps = setup_script_and_steps(command_line);
    let runs_on = runs_on_info.runs_on();
    let strategy = runs_on_info.strategy();
    Job { name, runs_on, steps, strategy, ..default() }
}

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
                "ide integration-test --project-manager-source current-ci-run",
            )
        }
    }

    pub struct BuildWasm;
    impl JobArchetype for BuildWasm {
        fn job(os: OS) -> Job {
            plain_job(&os, "Build GUI (WASM)", "wasm build")
        }
    }

    pub struct BuildProjectManager;
    impl JobArchetype for BuildProjectManager {
        fn job(os: OS) -> Job {
            plain_job(&os, "Build Project Manager", "project-manager")
        }
    }

    pub struct PackageIde;
    impl JobArchetype for PackageIde {
        fn job(os: OS) -> Job {
            plain_job(
                &os,
                "Package IDE",
                "ide build --wasm-source current-ci-run --project-manager-source current-ci-run",
            )
        }
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    pub struct DeluxeRunner;

    impl RunsOn for DeluxeRunner {
        fn runs_on(&self) -> Vec<RunnerLabel> {
            vec![RunnerLabel::MwuDeluxe]
        }

        fn os_name(&self) -> Option<String> {
            None
        }
    }

    #[test]
    fn generate_nightly_ci() -> Result {
        let on = Event {
            workflow_dispatch: Some(WorkflowDispatch {}),
            push: Some(Push { ..default() }),
            ..default()
        };


        let all_platforms = Strategy::new_os(
            [OS::Windows, OS::Linux, OS::MacOS].into_iter().map(|os| runs_on(os)).collect_vec(),
        );
        let linux_only = OS::Linux;


        let prepare_outputs = ["ENSO_VERSION", "ENSO_RELEASE_ID"];

        let prepare = {
            let name = "Prepare release".into();
            let runs_on = vec![RunnerLabel::Linux, RunnerLabel::MwuDeluxe];

            let prepare_step_id = "prepare";
            let mut prepare = shell_os(linux_only, "./run release create-draft");
            prepare.id = Some(prepare_step_id.into());

            let mut steps = setup_script_steps();
            steps.push(prepare);

            let mut ret = Job { name, runs_on, steps, ..default() };
            for output in prepare_outputs {
                ret.expose_output(prepare_step_id, output);
            }
            ret
        };


        let mut workflow = Workflow { name: "Nightly Release".into(), on, ..default() };
        let prepare_job_id = workflow.add_job(prepare);



        let build_wasm: Job = {
            let mut ret = plain_job(&linux_only, "Build WASM", "wasm build");
            workflow.expose_outputs(&prepare_job_id, &mut ret);
            ret
        };
        let build_wasm_job_id = workflow.add_job(build_wasm);

        let build_engine: Job = {
            let mut ret = plain_job(&all_platforms, "Build Backend", "backend upload");
            workflow.expose_outputs(&prepare_job_id, &mut ret);
            ret
        };
        let build_engine_job_id = workflow.add_job(build_engine);

        let build_ide: Job = {
            let mut ret = plain_job(&all_platforms, "Build IDE", "ide upload --wasm-source current-ci-run --backend-source release --backend-release ${{env.ENSO_RELEASE_ID}}");
            workflow.expose_outputs(&prepare_job_id, &mut ret);
            ret.needs(&build_wasm_job_id);
            ret.needs(&build_engine_job_id);
            ret
        };
        let build_ide_job_id = workflow.add_job(build_ide);


        let publish: Job = {
            let mut ret = plain_job(&linux_only, "Publish release", "release publish");
            workflow.expose_outputs(&prepare_job_id, &mut ret);
            ret.needs(&build_wasm_job_id);
            ret.needs(&build_engine_job_id);
            ret.needs(&build_ide_job_id);
            ret.expose_secret_as("ARTEFACT_S3_ACCESS_KEY_ID", "AWS_ACCESS_KEY_ID");
            ret.expose_secret_as("ARTEFACT_S3_SECRET_ACCESS_KEY ", "AWS_SECRET_ACCESS_KEY");
            ret.env("AWS_REGION", "us-west-1");
            ret
        };
        let _publish_job_id = workflow.add_job(publish);



        let global_env = [
            ("ENSO_BUILD_KIND", "nightly"),
            ("ENSO_BUILD_REPO_REMOTE", "enso-org/enso-staging"),
            ("RUST_BACKTRACE", "full"),
        ];
        for (var_name, value) in global_env {
            workflow.env(var_name, value);
        }


        let yaml = serde_yaml::to_string(&workflow)?;
        println!("{yaml}");
        let path = r"H:\NBO\enso-staging\.github\workflows\nightly.yml";
        crate::fs::write(path, yaml)?;
        Ok(())
    }

    #[test]
    fn generate_gui_ci() -> Result {
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
            job.needs.insert(job::BuildProjectManager::key(primary_os));
        });

        for os in [OS::Windows, OS::Linux, OS::MacOS] {
            let wasm_job = workflow.add::<job::BuildWasm>(os);
            let project_manager_job = workflow.add::<job::BuildProjectManager>(os);
            workflow.add_customized::<job::PackageIde>(os, |job| {
                job.needs.insert(wasm_job);
                job.needs.insert(project_manager_job);
            });
        }

        let yaml = serde_yaml::to_string(&workflow)?;
        println!("{yaml}");
        let path = r"H:\NBO\enso4\.github\workflows\gui.yml";
        crate::fs::write(path, yaml)?;
        Ok(())
    }
}
