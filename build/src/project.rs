use crate::prelude::*;

use crate::ide::web::GuiArtifacts;
use crate::ide::web::IdeDesktop;
use crate::paths::generated::RepoRoot;
use gui::Gui;
use gui::GuiInputs;
use ide_ci::actions::workflow::is_in_env;

pub mod gui;
pub mod project_manager;
pub mod wasm;


#[async_trait]
pub trait IsTarget: Sized {
    /// All the data needed to build this target that are not placed in `self`.
    type BuildInput: Send + 'static;

    /// A location-like value with the directory where the artifacts are placed.
    type Output: Clone + Send + Sync + AsRef<Path> + for<'a> From<&'a Path>;

    /// Identifier used when uploading build artifacts to run.
    ///
    /// Note that this is not related to the assets name in the release.
    fn artifact_name(&self) -> &str;

    async fn build(
        &self,
        input: Self::BuildInput,
        output_path: impl AsRef<Path> + Send + Sync + 'static,
    ) -> Result<Self::Output>;

    async fn get(
        &self,
        source: Source,
        get_inputs: impl FnOnce() -> Result<Self::BuildInput> + Send + 'static,
        output: PathBuf,
    ) -> Result<Self::Output> {
        match source {
            Source::BuildLocally => {
                return self.build(get_inputs()?, output).await;
            }
            Source::OngoingCiRun => {
                ide_ci::actions::artifacts::download_single_file_artifact(
                    self.artifact_name(),
                    output.as_path(),
                )
                .await?;
            }
            Source::LocalFile(source_dir) => {
                ide_ci::fs::mirror_directory(source_dir, output.as_path()).await?;
            }
        }
        Ok(Self::Output::from(&output))
    }

    async fn upload_artifact(&self, output: Self::Output) -> Result {
        if is_in_env() {
            ide_ci::actions::artifacts::upload_directory(output.as_ref(), self.artifact_name())
                .await?;
        }
        Ok(())
    }
}


pub enum Source {
    BuildLocally,
    OngoingCiRun,
    LocalFile(PathBuf),
}
