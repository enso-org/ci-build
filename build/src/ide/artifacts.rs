use crate::ide::web::GuiArtifacts;
use crate::ide::web::IdeDesktop;
use crate::ide::BuildInfo;
use crate::paths::generated::RepoRoot;
use crate::paths::generated::RepoRootDistWasm;
use crate::prelude::*;
use crate::project::wasm;
use crate::project::wasm::Artifacts;
use crate::project::IsTarget;
use aws_sdk_s3::ErrorExt;
use ide_ci::actions::workflow::is_in_env;
use octocrab::models::RunId;
use std::error::Error;
use std::marker::PhantomData;

//
// pub mod eee {
//     use super::*;
//
//     pub trait Variable {
//         const NAME: &'static str;
//         type Value;
//
//         fn parse(value: OsString) -> Result<Self::Value>;
//         fn generate(value: &Self::Value) -> Cow<OsStr>;
//     }
// }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::wasm::Wasm;
    use crate::project::IsTarget;
    use crate::project::IsTarget;

    #[tokio::test]
    async fn aaa() -> Result {
        let a = String::new();
        let f = Wasm.build(todo!(), a.as_str());
        let ret = tokio::task::spawn(f).await??;
        Ok(())
    }
}
