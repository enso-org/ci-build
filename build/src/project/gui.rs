use crate::prelude::*;

use crate::ide::web::IdeDesktop;
use crate::paths::generated::RepoRoot;
use crate::project::wasm;
use crate::project::IsArtifact;
use crate::project::IsTarget;
use crate::project::IsWatchable;
use crate::project::PlainArtifact;
use crate::BoxFuture;

pub type Artifact = PlainArtifact<Gui>;

#[derive(derivative::Derivative)]
#[derivative(Debug)]
pub struct GuiInputs {
    pub repo_root:  RepoRoot,
    #[derivative(Debug = "ignore")]
    pub wasm:       BoxFuture<'static, Result<wasm::Artifact>>,
    #[derivative(Debug = "ignore")]
    pub build_info: BoxFuture<'static, Result<BuildInfo>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Gui;

#[async_trait]
impl IsTarget for Gui {
    type BuildInput = GuiInputs;
    type Artifact = Artifact;

    fn artifact_name(&self) -> &str {
        "gui"
    }

    fn build(
        &self,
        input: Self::BuildInput,
        output_path: impl AsRef<Path> + Send + Sync + 'static,
    ) -> BoxFuture<'static, Result<Self::Artifact>> {
        async move {
            let ide = IdeDesktop::new(&input.repo_root.app.ide_desktop);
            ide.build_content(input.wasm, &input.build_info.await?, &output_path).await?;
            Ok(Artifact::new(output_path.as_ref()))
        }
        .boxed()
    }
}

impl IsWatchable for Gui {
    type Watcher = crate::project::Watcher<Self, crate::ide::web::Watcher>;

    fn setup_watcher(
        &self,
        input: Self::BuildInput,
        output_path: impl AsRef<Path> + Send + Sync + 'static,
    ) -> BoxFuture<'static, Result<Self::Watcher>> {
        async move {
            let ide = IdeDesktop::new(&input.repo_root.app.ide_desktop);
            let watch_process = ide.watch_content(input.wasm, &input.build_info.await?).await?;
            let artifact = Self::Artifact::from_existing(output_path).await?;
            Ok(Self::Watcher { watch_process, artifact })
        }
        .boxed()
    }
}



#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildInfo {
    pub commit:         String,
    pub version:        Version,
    pub engine_version: Version,
    pub name:           String,
}
