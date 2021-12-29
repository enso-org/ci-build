use crate::prelude::*;
use crate::programs::cmd;
use crate::programs::vswhere::VsWhere;

pub struct Cl;

impl Program for Cl {
    fn executable_name() -> &'static str {
        "cl"
    }
}

pub async fn apply_dev_environment() -> Result {
    let msvc = VsWhere::msvc().await?;
    let path =
        msvc.installation_path.join("VC").join("Auxiliary").join("Build").join("vcvarsall.bat");
    let changes = cmd::compare_env(|command| {
        command.arg(path).arg("x64").env("VSCMD_SKIP_SENDTELEMETRY", "true")
    })
    .await?;
    for change in changes {
        change.apply()?;
    }
    Ok(())
}

/// Serialization follows the VS Where `productLineVersion` format.
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub enum Version {
    #[serde(rename = "2017")]
    VS2017,
    #[serde(rename = "2019")]
    VS2019,
    #[serde(rename = "2022")]
    VS2022,
}

#[tokio::test]
async fn foo() -> Result {
    // let old_vars = dbg!(std::env::vars_os().map(|(name, _)| name).collect_vec());
    apply_dev_environment().await?;
    // let new_vars = dbg!(std::env::vars_os().collect_vec());
    Ok(())
}
