pub mod download;

use crate::prelude::*;
use anyhow::Context;
use reqwest::Client;
use reqwest::IntoUrl;

use crate::io::filename_from_url;
use crate::io::web::stream_response_to_file;
use serde::de::DeserializeOwned;
use sha2::Digest;

pub const VERSION: u8 = 1;

pub trait Storable: Debug {
    type Metadata: Serialize + DeserializeOwned;
    type Output;

    fn digest(&self, digest: &mut impl Digest) -> Result;

    fn generate(&self, cache: Cache, store: PathBuf) -> BoxFuture<'static, Result<Self::Metadata>>;

    fn adapt(
        &self,
        cache: PathBuf,
        metadata: Self::Metadata,
    ) -> BoxFuture<'static, Result<Self::Output>>;
}

pub trait IsKey: PartialEq + Serialize + Debug + DeserializeOwned + Hash {}
impl<T: PartialEq + Serialize + Debug + DeserializeOwned + Hash> IsKey for T {}

#[derive(Clone, Debug)]
pub struct Cache {
    root: PathBuf,
}

impl Cache {
    pub async fn new(path: impl Into<PathBuf>) -> Result<Self> {
        let root = path.into();
        crate::fs::tokio::create_dir_if_missing(&root).await?;
        Ok(Self { root })
    }

    #[tracing::instrument]
    pub async fn get<Key>(&self, key: Key) -> Result<Key::Output>
    where Key: Storable {
        let mut digest = sha2::Sha224::default();
        sha2::Digest::update(&mut digest, &[VERSION]);
        key.digest(&mut digest)?;
        let digest = digest.finalize();
        let code = data_encoding::BASE64URL_NOPAD.encode(&digest);

        let entry_dir = self.root.join(&code);
        crate::fs::tokio::create_dir_if_missing(&entry_dir).await?;

        let complete_marker = entry_dir.with_appended_extension("json");

        let is_ready = || complete_marker.exists();

        if !is_ready() {
            debug!("Not found in cache, will generate.");
            let metadata = key.generate(self.clone(), entry_dir.clone()).await?;
            complete_marker.write_as_json(&metadata)?;
        } else {
            debug!("Found in cache, skipping generation.");
        }

        let metadata = complete_marker.read_to_json().context("Reading metadata.")?;
        key.adapt(entry_dir, metadata).await
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::download::DownloadFile;
    use crate::log::setup_logging;

    #[tokio::test]
    async fn cache_test() -> Result {
        setup_logging()?;
        let download_task = DownloadFile::new("https://store.akamai.steamstatic.com/public/shared/images/header/logo_steam.svg?t=962016")?;

        let cache = Cache::new("C:/temp/enso-cache").await?;
        cache.get(download_task).await?;


        Ok(())
    }
}
