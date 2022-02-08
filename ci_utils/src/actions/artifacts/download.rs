use crate::actions::artifacts::models::ContainerEntry;
use crate::prelude::*;
use anyhow::Context;

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
        let path_without_name = entry
            .path
            .strip_prefix(artifact_name)
            .context("Entry path does not start with an artifact name.")?;
        ensure!(entry.path.is_absolute(), "Path {} is not absolute.", entry.path.display());
        let path_without_name_and_following_separator = path_without_name
            .strip_prefix("/")
            .or_else(|_| path_without_name.strip_prefix("\\"))
            .context("Artifact path is invalid: should be followed by a separator.")?;

        Ok(Self {
            target:                 target_root
                .as_ref()
                .join(path_without_name_and_following_separator),
            remote_source_location: entry.content_location.clone(),
        })
    }
}
