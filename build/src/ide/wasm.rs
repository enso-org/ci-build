use crate::prelude::*;
use anyhow::Context;
use ide_ci::actions::workflow::is_in_env;
use octocrab::models::RunId;
use octocrab::params::actions::ArchiveFormat;

use crate::ide::wasm::js_patcher::patch_js_glue_in_place;
use crate::paths::generated::Parameters;
use crate::paths::generated::Paths;
use crate::paths::generated::PathsRepoRootDistWasm;

use ide_ci::env::Variable;
use ide_ci::models::config::RepoContext;
use ide_ci::programs::WasmPack;

pub mod js_patcher;


const WASM_ARTIFACT_NAME: &str = "ide-wasm";
const OUTPUT_NAME: &str = "ide";
const TARGET_CRATE: &str = "app/gui";

pub struct WasmArtifacts {
    pub dir:     PathBuf,
    pub wasm:    PathBuf,
    pub js_glue: PathBuf,
}

pub async fn build_wasm(
    repo_root: impl AsRef<Path>,
    output_dir: &PathsRepoRootDistWasm,
) -> Result<WasmArtifacts> {
    ide_ci::fs::create_dir_if_missing(&output_dir.path)?;
    ide_ci::programs::WasmPack
        .cmd()?
        .env_remove(ide_ci::programs::rustup::env::Toolchain::NAME)
        .args([
            "-vv",
            "build",
            "--target",
            "web",
            "--out-dir",
            output_dir.path.as_str(), // &paths.wasm().as_os_str().to_str().unwrap(),
            "--out-name",
            OUTPUT_NAME,
            TARGET_CRATE,
        ])
        .current_dir(&repo_root)
        .spawn()?
        .wait()
        .await?
        .exit_ok()?;

    patch_js_glue_in_place(&output_dir.wasm_glue)?;
    ide_ci::fs::rename(&output_dir.wasm_main_raw, &output_dir.wasm_main)?;

    if is_in_env() {
        ide_ci::actions::artifacts::upload_directory(&output_dir, "ide_wasm").await?;
    }

    Ok(WasmArtifacts {
        dir:     output_dir.path.clone(),
        wasm:    output_dir.wasm_main.to_path_buf(),
        js_glue: output_dir.wasm_glue.to_path_buf(),
    })
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

pub enum WasmSource {
    Build(Paths),
    Local(PathBuf),
    GuiCiRun { repo: RepoContext, run: RunId },
}

impl WasmSource {
    pub async fn place_at(&self, client: &Octocrab, output_dir: impl AsRef<Path>) -> Result {
        match self {
            WasmSource::Build(repo_root) => {
                let faux_parameters = Parameters {
                    repo_root: repo_root.into(),
                    triple:    "".into(),
                    temp:      "".into(),
                };
                let wasm_dir = PathsRepoRootDistWasm::new2(&faux_parameters, output_dir.as_ref());
                build_wasm(repo_root, &wasm_dir).await?;
            }
            WasmSource::Local(local_path) => {
                ide_ci::fs::copy(local_path, &output_dir)?;
            }
            WasmSource::GuiCiRun { repo, run } => {
                download_wasm_from_run(client, &repo, *run, &output_dir).await?;
            }
        }
        Ok(())
    }
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
