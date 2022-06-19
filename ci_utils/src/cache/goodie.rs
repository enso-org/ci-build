use crate::prelude::*;

use crate::cache;
use crate::cache::Cache;

pub mod binaryen;

/// Something that can be downloaded and, after that, enabled by modifying global state.
pub trait Goodie: Debug + Clone + Send + Sync + 'static {
    fn url(&self) -> Result<Url>;
    fn enable(&self, package_path: PathBuf) -> Result;
}

pub trait GoodieExt: Goodie {
    fn install_if_missing<P: Program + Sync>(
        &self,
        cache: &Cache,
        program: P,
    ) -> BoxFuture<'static, Result> {
        let program_name = program.executable_name().to_owned();
        let check_presence = program.require_present();
        let this = self.clone();
        let cache = cache.clone();
        async move {
            if check_presence.await.is_ok() {
                debug!("Skipping install of {this:?} because {program_name} is present.",);
                Ok(())
            } else {
                let package_path = this.download(&cache).await?;
                this.enable(package_path)
            }
        }
        .boxed()
    }

    fn download(&self, cache: &Cache) -> BoxFuture<'static, Result<PathBuf>> {
        let get_job = (|| {
            let url = self.url()?;
            let archive_source = cache::download::DownloadFile::new(url)?;
            let path_to_extract = None;
            let extracted = cache::archive::ExtractedArchive { archive_source, path_to_extract };
            Result::Ok(cache.get(extracted))
        })();
        get_job.flatten_fut().boxed()
    }
}

impl<T: Goodie> GoodieExt for T {}
