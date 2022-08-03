use crate::cache;
use crate::prelude::*;
use crate::programs;

const DOWNLOAD_URL_TEXT: &str = "https://github.com/sbt/sbt/releases/download/v1.5.5/sbt-1.5.5.tgz";


#[derive(Debug, Clone, PartialEq, Display)]
pub struct Sbt;

impl cache::Goodie for Sbt {
    fn url(&self) -> BoxFuture<'static, Result<Url>> {
        ready(Url::parse(DOWNLOAD_URL_TEXT).anyhow_err()).boxed()
    }

    fn is_active(&self) -> BoxFuture<'static, Result<bool>> {
        ready(Ok(programs::Sbt.lookup().is_ok())).boxed()
    }

    fn activate(&self, package_path: PathBuf) -> Result {
        crate::env::prepend_to_path(package_path.join("bin"))
    }
}
