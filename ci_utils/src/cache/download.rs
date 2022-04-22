use crate::prelude::*;

use crate::cache::Cache;
use crate::cache::Storable;
use crate::io::filename_from_url;
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
            let filename = filename?;
            let response = crate::io::web::handle_error_response(response.await?).await?;
            let output = store.join(&filename);
            stream_response_to_file(response, &output).await?;
            Ok(filename) // We don't store absolute paths to keep cache relocatable.
        }
        .boxed()
    }

    fn adapt(
        &self,
        cache: PathBuf,
        metadata: Self::Metadata,
    ) -> BoxFuture<'static, Result<Self::Output>> {
        let path = cache.join(metadata);
        crate::fs::tokio::open(path).boxed()
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;
    use headers::Header;
    use headers::HeaderValue;
    use regex::Regex;
    use reqwest::header::CONTENT_DISPOSITION;
    use reqwest::Response;

    pub fn filename_from_content_disposition(value: &HeaderValue) -> Result<&Path> {
        let regex = Regex::new(r#"filename="?([^"]*)"?"#)?;
        let capture = regex
            .captures(value.to_str()?)
            .context("Field 'filename' not present in the header value.")?
            .get(1)
            .context("Missing capture group from regex.")?;
        Ok(&Path::new(capture.as_str()))
    }

    pub fn filename_from_response(response: &Response) -> Result<&Path> {
        let disposition = response
            .headers()
            .get(CONTENT_DISPOSITION)
            .context(format!("No {CONTENT_DISPOSITION} header present in the response."))?;
        filename_from_content_disposition(disposition)
    }

    #[tokio::test]
    async fn download_header() -> Result {
        let response = reqwest::get(
            "https://github.com/enso-org/enso/releases/download/enso-0.2.31/launcher-manifest.yaml",
        )
        .await?;
        dbg!(&response);
        dbg!(filename_from_response(&response));

        Ok(())
    }
}
