use crate::actions::artifacts::models::ContainerEntry;
use crate::actions::artifacts::API_VERSION;
use crate::prelude::*;
// use anyhow::Context;
use reqwest::header::HeaderMap;
use reqwest::header::HeaderValue;
use reqwest::header::ACCEPT;
use reqwest::header::ACCEPT_ENCODING;

#[derive(Clone, Debug)]
pub struct FileToDownload {
    /// Absolute path in the local filesystem.
    pub target:                 PathBuf,
    /// Relative path within the artifact container. Does not include the leading segment with the
    /// artifact name.
    pub remote_source_location: Url,
}

impl FileToDownload {
    #[context("Failed to process entry {} from artifact {}.", entry.path.display(), artifact_name)]
    pub fn new(
        target_root: impl AsRef<Path>,
        entry: &ContainerEntry,
        artifact_name: &str,
    ) -> Result<Self> {
        ensure!(entry.path.is_relative(), "Path {} is not relative.", entry.path.display());
        let mut path_iter = entry.path.iter();
        ensure!(
            path_iter.next() == Some(&OsStr::new(artifact_name)),
            "Entry path does not start with an artifact name."
        );

        Ok(Self {
            target:                 target_root.as_ref().join(path_iter),
            remote_source_location: entry.content_location.clone(),
        })
    }
}

pub fn headers() -> HeaderMap {
    let mut header = HeaderMap::new();
    // We can safely unwrap, because we know that all mime types are in format that can be used
    // as HTTP header value.
    header.insert(ACCEPT_ENCODING, HeaderValue::from_static("gzip"));
    header.insert(
        ACCEPT,
        HeaderValue::try_from(format!(
            "{};api-version={}",
            mime::APPLICATION_OCTET_STREAM,
            API_VERSION
        ))
        .unwrap(),
    );
    header
}
