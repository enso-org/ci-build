use crate::prelude::*;

use crate::ide::web::IdeDesktop;
use crate::paths::generated::RepoRoot;
use crate::project::ide::BuildInfo;
use crate::project::wasm;
use crate::project::IsArtifact;
use crate::project::IsTarget;
use crate::project::PlainArtifact;
use crate::project::Source;
use crate::BoxFuture;

pub type Artifact = PlainArtifact<Gui>;

pub struct GuiInputs {
    pub repo_root:  RepoRoot,
    pub wasm:       BoxFuture<'static, Result<wasm::Artifacts>>,
    pub build_info: BuildInfo,
}

#[derive(Clone, Debug)]
pub struct Gui;

impl Gui {
    pub async fn watch(&self, input: GuiInputs) -> Result {
        let ide = IdeDesktop::new(&input.repo_root.app.ide_desktop);
        ide.watch(input.wasm, &input.build_info).await?;
        Ok(())
    }
}

#[async_trait]
impl IsTarget for Gui {
    type BuildInput = GuiInputs;
    type Output = Artifact;

    fn artifact_name(&self) -> &str {
        "gui"
    }


    fn build(
        &self,
        input: Self::BuildInput,
        output_path: impl AsRef<Path> + Send + Sync + 'static,
    ) -> BoxFuture<'static, Result<Self::Output>> {
        // We cannot just clone the build future, because Error (and thus Result) is not cloneable.
        // So we'll share the the OK result through a oneshot channel.
        // The idea is that upload artifact task can be run independently.
        let (tx, rx) = tokio::sync::oneshot::channel();
        let ret = async move {
            let ide = IdeDesktop::new(&input.repo_root.app.ide_desktop);
            ide.build(input.wasm, &input.build_info, &output_path).await?;
            let artifacts = Artifact::new(output_path.as_ref());
            // We ignore error, because we don't care if upload task is actually run.
            let _ = tx.send(artifacts.clone());
            Ok(artifacts)
        };

        let upload_job = self.upload_artifact(rx.map_err(into));
        tokio::spawn(upload_job);
        ret.boxed()
    }
}
