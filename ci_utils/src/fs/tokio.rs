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
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            trace!("Directory already exists: {}", path.as_ref().display());
            Ok(())
        }
        result => {
            trace!("Created directory: {}", path.as_ref().display());
            result.anyhow_err()
        }
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
    let mut output = create(output_path).await?;
    tokio::io::copy(&mut content, &mut output).await.anyhow_err()
}

/// Remove a directory with all its subtree.
///
/// Does not fail if the directory is not found.
#[instrument(fields(path = %path.as_ref().display()), err, level = "trace")]
pub async fn remove_dir_if_exists(path: impl AsRef<Path>) -> Result {
    let path = path.as_ref();
    let result = tokio::fs::remove_dir_all(&path).await;
    match result {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        result => result.context(format!("Failed to remove directory {}.", path.display())),
    }
}

/// Recreate directory, so it exists and is empty.
pub async fn reset_dir(path: impl AsRef<Path>) -> Result {
    let path = path.as_ref();
    remove_dir_if_exists(&path).await?;
    create_dir_if_missing(&path).await?;
    Ok(())
}
