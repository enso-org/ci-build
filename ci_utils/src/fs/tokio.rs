use crate::prelude::*;

use tokio::fs::File;
use tokio::io::AsyncRead;

pub use crate::fs::wrappers::tokio::*;

/// Like the standard version but will create any missing parent directories from the path.
#[context("Failed to open path for writing: {}", path.as_ref().display())]
pub async fn create(path: impl AsRef<Path>) -> Result<File> {
    create_parent_dir_if_missing(&path).await?;
    crate::fs::wrappers::tokio::create(&path).await
}

/// Create a directory (and all missing parent directories),
///
/// Does not fail when a directory already exists.
#[context("Failed to create directory {}", path.as_ref().display())]
pub async fn create_dir_if_missing(path: impl AsRef<Path>) -> Result {
    let result = tokio::fs::create_dir_all(&path).await;
    match result {
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        result => result.anyhow_err(),
    }
}

/// Create a parent directory for path (and all missing parent directories),
///
/// Does not fail when a directory already exists.
#[context("Failed to create parent directory for {}", path.as_ref().display())]
pub async fn create_parent_dir_if_missing(path: impl AsRef<Path>) -> Result<PathBuf> {
    if let Some(parent) = path.as_ref().parent() {
        create_dir_if_missing(parent).await?;
        Ok(parent.into())
    } else {
        bail!("No parent directory for path {}.", path.as_ref().display())
    }
}

pub async fn copy_to_file(
    mut content: impl AsyncRead + Unpin,
    output_path: impl AsRef<Path>,
) -> Result<u64> {
    let mut output = crate::fs::tokio::create(output_path).await?;
    tokio::io::copy(&mut content, &mut output).await.anyhow_err()
}
