use crate::ide::web::IdeDesktop;
use crate::ide::BuildInfo;
use crate::paths::generated::RepoRoot;
use crate::project::wasm;
use crate::BoxFuture;
use ide_ci::prelude;

pub struct GuiInputs {
    pub repo_root:  RepoRoot,
    pub wasm:       BoxFuture<'static, wasm::Artifacts>,
    pub build_info: BuildInfo,
}

#[derive(Clone, Debug)]
pub struct Gui;

impl Gui {
    pub async fn watch(&self, input: GuiInputs) -> prelude::Result {
        let ide = IdeDesktop::new(&input.repo_root.app.ide_desktop);
        ide.watch(&input.wasm, &input.build_info).await?;
        Ok(())
    }
}

#[async_trait]
impl IsTarget for Gui {
    type BuildInput = GuiInputs;
    type Output = GuiArtifacts;

    fn artifact_name(&self) -> &str {
        "gui_wasm"
    }

    async fn build(
        &self,
        input: Self::BuildInput,
        output_path: impl AsRef<Path> + Send + Sync + 'static,
    ) -> Result<Self::Output> {
        let ide = IdeDesktop::new(&input.repo_root.app.ide_desktop);
        let ret = ide.build(&input.wasm, &input.build_info, output_path).await?;
        let upload_task = self.upload_artifact(ret.clone());
        tokio::spawn(upload_task);
    }
}
