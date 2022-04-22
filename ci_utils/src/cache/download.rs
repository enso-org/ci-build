use crate::prelude::*;

use crate::cache::Cache;
use crate::cache::Storable;
use crate::io::filename_from_url;
use crate::io::web::filename_from_response;
use crate::io::web::stream_response_to_file;

use reqwest::Client;
use reqwest::IntoUrl;
use sha2::Digest;

#[derive(Clone, Debug)]
pub struct DownloadFile {
    pub url:    Url,
    pub client: Client,
}

impl DownloadFile {
    pub fn new(url: impl IntoUrl) -> Result<Self> {
        Ok(Self { url: url.into_url()?, client: default() })
    }
}

impl Storable for DownloadFile {
    type Metadata = PathBuf;
    type Output = tokio::fs::File;

    fn digest(&self, digest: &mut impl Digest) -> Result {
        digest.update(self.url.as_str().as_bytes());
        Ok(())
    }

    fn generate(
        &self,
        _cache: Cache,
        store: PathBuf,
    ) -> BoxFuture<'static, Result<Self::Metadata>> {
        let response = self.client.get(self.url.clone()).send();
        let filename = filename_from_url(&self.url);
        async move {
            let last_fallback_name = PathBuf::from("data");
            let response = crate::io::web::handle_error_response(response.await?).await?;
            let filename = filename_from_response(&response)
                .map(ToOwned::to_owned)
                .or(filename)
                .unwrap_or(last_fallback_name);
            let output = store.join(&filename);
            stream_response_to_file(response, &output).await?;
            Ok(filename) // We don't store absolute paths to keep cache relocatable.
        }
        .boxed()
    }

    fn adapt(
        &self,
        store: PathBuf,
        metadata: Self::Metadata,
    ) -> BoxFuture<'static, Result<Self::Output>> {
        let path = store.join(metadata);
        crate::fs::tokio::open(path).boxed()
    }
}
