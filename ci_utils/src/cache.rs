pub mod archive;
pub mod artifact;
pub mod asset;
pub mod download;
pub mod goodie;

use crate::prelude::*;
use anyhow::Context;
use std::hash::Hasher;

use serde::de::DeserializeOwned;
use sha2::Digest;

pub use goodie::Goodie;

pub const VERSION: u8 = 1;

pub fn default_path() -> Result<PathBuf> {
    Ok(dirs::home_dir().context("Cannot locate home directory.")?.join_iter([".enso-ci", "cache"]))
}

pub trait Storable: Debug + Send + Sync + 'static {
    type Metadata: Serialize + DeserializeOwned + Send + Sync + 'static;
    type Output: Clone + Send + Sync + 'static;
    type Key: Clone + Debug + Serialize + DeserializeOwned + Send + Sync + 'static;

    fn generate(&self, cache: Cache, store: PathBuf) -> BoxFuture<'static, Result<Self::Metadata>>;

    fn adapt(
        &self,
        cache: PathBuf,
        metadata: Self::Metadata,
    ) -> BoxFuture<'static, Result<Self::Output>>;

    fn key(&self) -> Self::Key;
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntryIndex<S: Storable> {
    pub metadata: S::Metadata,
    pub key:      S::Key,
}

pub struct HashToDigest<'a, D: Digest>(&'a mut D);
impl<'a, D: Digest> Hasher for HashToDigest<'a, D> {
    fn finish(&self) -> u64 {
        todo!()
    }

    fn write(&mut self, bytes: &[u8]) {
        self.0.update(bytes)
    }
}

pub fn digest<S: Storable>(storable: &S) -> Result<String> {
    let key = storable.key();
    let key_serialized = bincode::serialize(&key)?;

    let mut digest = sha2::Sha224::default();
    sha2::Digest::update(&mut digest, &[VERSION]);
    sha2::Digest::update(&mut digest, &key_serialized);
    std::any::TypeId::of::<S::Key>().hash(&mut HashToDigest(&mut digest));
    std::any::TypeId::of::<S>().hash(&mut HashToDigest(&mut digest));
    let digest = digest.finalize();
    Ok(data_encoding::BASE64URL_NOPAD.encode(&digest))
}

#[derive(Clone, Debug)]
pub struct Cache {
    root: PathBuf,
}

impl Cache {
    pub async fn new_default() -> Result<Self> {
        Self::new(default_path()?).await
    }

    pub async fn new(path: impl Into<PathBuf>) -> Result<Self> {
        let root = path.into();
        crate::fs::tokio::create_dir_if_missing(&root).await?;
        debug!("Prepared cache in {}", root.display());
        Ok(Self { root })
    }

    pub fn get<S>(&self, storable: S) -> BoxFuture<'static, Result<S::Output>>
    where S: Storable {
        let this = self.clone();
        async move {
            // FIXME trace
            let code = digest(&storable)?;
            let entry_dir = this.root.join(&code);
            let entry_meta = entry_dir.with_appended_extension("json");

            let retrieve = async {
                let info = entry_meta.read_to_json::<EntryIndex<S>>()?;
                crate::fs::require_exist(&entry_dir)?;
                storable.adapt(entry_dir.clone(), info.metadata).await
            };

            match retrieve.await {
                Ok(out) => {
                    debug!("Found in cache, skipping generation.");
                    Ok(out)
                }
                Err(e) => {
                    debug!("Value cannot be retrieved from cache because: {e}");
                    crate::fs::reset_dir(&entry_dir)?;
                    let key = storable.key();
                    let info = EntryIndex::<S> {
                        metadata: storable
                            .generate(this, entry_dir.clone())
                            .instrument(info_span!("Generating value to be cached.", ?key))
                            .await?,
                        key:      key.clone(),
                    };
                    entry_meta.write_as_json(&info)?;
                    storable.adapt(entry_dir, info.metadata).await
                }
            }
        }
        .boxed()
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
