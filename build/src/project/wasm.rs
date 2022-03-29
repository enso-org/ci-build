use crate::prelude::*;
use anyhow::Context;
use octocrab::models::RunId;
use octocrab::params::actions::ArchiveFormat;

use crate::project::wasm::js_patcher::patch_js_glue_in_place;
// use crate::paths::generated::Parameters;
// use crate::paths::generated::Paths;
// use crate::paths::generated::PathsRepoRootDistWasm;

use crate::paths::generated::RepoRoot;
use crate::paths::generated::RepoRootDistWasm;
use crate::project::IsArtifact;
use crate::project::IsTarget;
use ide_ci::env::Variable;
use ide_ci::models::config::RepoContext;

pub mod js_patcher;


const WASM_ARTIFACT_NAME: &str = "ide-wasm";
const OUTPUT_NAME: &str = "ide";
const TARGET_CRATE: &str = "app/gui";

#[derive(Clone, Debug)]
pub struct Wasm;

#[async_trait]
impl IsTarget for Wasm {
    type BuildInput = RepoRoot;
    type Output = Artifacts;

    fn artifact_name(&self) -> &str {
        "gui_wasm"
    }

    fn build(
        &self,
        input: Self::BuildInput,
        output_path: impl AsRef<Path> + Send + Sync + 'static,
    ) -> BoxFuture<'static, Result<Self::Output>> {
        build_wasm(input, output_path).boxed()
    }
}



#[derive(Clone, Debug, Display)]
pub struct Artifacts(RepoRootDistWasm);

impl Artifacts {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self(RepoRootDistWasm::new(path))
    }
    pub fn wasm(&self) -> &Path {
        &self.0.wasm_main
    }
    pub fn js_glue(&self) -> &Path {
        &self.0.wasm_glue
    }
    pub fn dir(&self) -> &Path {
        &self.0.path
    }
}

impl AsRef<Path> for Artifacts {
    fn as_ref(&self) -> &Path {
        self.0.as_path()
    }
}

impl IsArtifact for Artifacts {
    fn from_existing(path: impl AsRef<Path>) -> BoxFuture<'static, Result<Self>> {
        ready(Ok(Artifacts::new(path.as_ref()))).boxed()
    }
}

pub async fn build_wasm(
    repo_root: impl AsRef<Path>,
    output_dir: impl AsRef<Path>,
) -> Result<Artifacts> {
    ide_ci::fs::create_dir_if_missing(&output_dir)?;
    ide_ci::programs::WasmPack
        .cmd()?
        .env_remove(ide_ci::programs::rustup::env::Toolchain::NAME)
        .args([
            "-vv",
            "build",
            "--target",
            "web",
            "--out-dir",
            output_dir.as_str(),
            "--out-name",
            OUTPUT_NAME,
            TARGET_CRATE,
        ])
        .current_dir(&repo_root)
        .spawn()?
        .wait()
        .await?
        .exit_ok()?;

    let ret = RepoRootDistWasm::new(output_dir.as_ref());
    patch_js_glue_in_place(&ret.wasm_glue)?;
    ide_ci::fs::rename(&ret.wasm_main_raw, &ret.wasm_main)?;
    let ret = Artifacts(ret);
    Ok(ret)
}

#[derive(Clone, Debug)]
pub enum WasmSource {
    Build { repo_root: PathBuf },
    Local(PathBuf),
    GuiCiRun { repo: RepoContext, run: RunId },
}

impl WasmSource {
    pub async fn place_at(
        &self,
        client: &Octocrab,
        output_dir: &RepoRootDistWasm,
    ) -> Result<Artifacts> {
        match self {
            WasmSource::Build { repo_root } => {
                build_wasm(repo_root, output_dir).await?;
            }
            WasmSource::Local(local_path) => {
                ide_ci::fs::copy(local_path, output_dir)?;
            }
            WasmSource::GuiCiRun { repo, run } => {
                download_wasm_from_run(client, &repo, *run, output_dir).await?;
            }
        }
        Ok(Artifacts::new(output_dir))
    }
}

// "Failed to find artifacts for run {run} in {repo}."
pub async fn download_wasm_from_run(
    client: &Octocrab,
    repo: &RepoContext,
    run: RunId,
    output_path: impl AsRef<Path>,
) -> Result {
    let artifacts = client
        .actions()
        .list_workflow_run_artifacts(&repo.owner, &repo.name, run)
        .per_page(100)
        .send()
        .await?
        .value
        .context(format!("Failed to find any artifacts."))?;

    let wasm_artifact = artifacts
        .into_iter()
        .find(|artifact| artifact.name == WASM_ARTIFACT_NAME)
        .context(format!("Failed to find artifact by name {WASM_ARTIFACT_NAME}"))?;

    let wasm = client
        .actions()
        .download_artifact(&repo.owner, &repo.name, wasm_artifact.id, ArchiveFormat::Zip)
        .await?;
    let wasm = std::io::Cursor::new(wasm);
    let mut wasm = zip::ZipArchive::new(wasm)?;

    ide_ci::fs::create_dir_if_missing(&output_path)?;
    wasm.extract(&output_path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup_octocrab;


    #[tokio::test]
    async fn build() -> Result {
        Ok(())
    }

    #[tokio::test]
    async fn test_artifact_download() -> Result {
        let out = r"C:\temp\wasm";
        let client = setup_octocrab()?;
        // let client = OctocrabBuilder::new()
        //     .personal_token("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".into())
        //     .build()?;
        let repo = RepoContext { owner: "enso-org".into(), name: "enso".into() };
        // https://github.com/enso-org/enso/actions/runs/1982165517
        download_wasm_from_run(&client, &repo, RunId(1982165517), out).await?;
        Ok(())
    }
}
