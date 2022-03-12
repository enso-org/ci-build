use crate::prelude::*;
use anyhow::Context;
use fs_extra::dir::CopyOptions;
use platforms::TARGET_OS;

pub mod wrappers;

pub use wrappers::*;

use std::fs::File;

/////////////////////////////

/// Like the standard version but will create any missing parent directories from the path.
#[context("Failed to write path: {}", path.as_ref().display())]
pub fn write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> Result {
    create_parent_dir_if_missing(&path)?;
    wrappers::write(&path, &contents)
}

/// Like the standard version but will create any missing parent directories from the path.
#[context("Failed to open path for writing: {}", path.as_ref().display())]
pub fn create(path: impl AsRef<Path>) -> Result<File> {
    create_parent_dir_if_missing(&path)?;
    wrappers::create(&path)
}

///////////////////////////

#[context("Failed to read the file: {}", path.as_ref().display())]
pub fn read_string_into<T: FromString>(path: impl AsRef<Path>) -> Result<T> {
    read_to_string(&path)?.parse2()
}

/// Create a directory (and all missing parent directories),
///
/// Does not fail when a directory already exists.
#[context("Failed to create directory {}", path.as_ref().display())]
pub fn create_dir_if_missing(path: impl AsRef<Path>) -> Result {
    let result = std::fs::create_dir_all(&path);
    match result {
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        result => result.anyhow_err(),
    }
}

/// Create a parent directory for path (and all missing parent directories),
///
/// Does not fail when a directory already exists.
pub fn create_parent_dir_if_missing(path: impl AsRef<Path>) -> Result<PathBuf> {
    if let Some(parent) = path.as_ref().parent() {
        create_dir_if_missing(parent)?;
        Ok(parent.into())
    } else {
        bail!("No parent directory for path {}.", path.as_ref().display())
    }
}

/// Remove a directory with all its subtree.
///
/// Does not fail if the directory is already gone.
#[context("Failed to remove directory {}", path.as_ref().display())]
pub fn remove_dir_if_exists(path: impl AsRef<Path>) -> Result {
    let result = std::fs::remove_dir_all(&path);
    match result {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        result => result.anyhow_err(),
    }
}

/// Remove a regular file.
///
/// Does not fail if the file is already gone.
#[context("Failed to remove file {}", path.as_ref().display())]
pub fn remove_file_if_exists(path: impl AsRef<Path>) -> Result<()> {
    let result = std::fs::remove_file(&path);
    match result {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        result => result.anyhow_err(),
    }
}


pub fn remove_if_exists(path: impl AsRef<Path>) -> Result {
    let path = path.as_ref();
    if path.is_dir() {
        remove_dir_if_exists(path)
    } else {
        remove_file_if_exists(path)
    }
}

/// Recreate directory, so it exists and is empty.
pub fn reset_dir(path: impl AsRef<Path>) -> Result {
    let path = path.as_ref();
    println!("Will reset directory {}", path.display());
    remove_dir_if_exists(&path)?;
    create_dir_if_missing(&path)?;
    Ok(())
}

pub fn require_exist(path: impl AsRef<Path>) -> Result {
    if path.as_ref().exists() {
        println!("{} does exist.", path.as_ref().display());
        Ok(())
    } else {
        bail!("{} does not exist.", path.as_ref().display())
    }
}

pub fn copy_to(source_file: impl AsRef<Path>, dest_dir: impl AsRef<Path>) -> Result {
    require_exist(&source_file)?;
    create_dir_if_missing(dest_dir.as_ref())?;
    println!("Will copy {} to {}", source_file.as_ref().display(), dest_dir.as_ref().display());
    let mut options = CopyOptions::new();
    options.overwrite = true;
    fs_extra::copy_items(&[source_file], dest_dir, &options)?;
    Ok(())
}

pub fn copy(source_file: impl AsRef<Path>, destination_file: impl AsRef<Path>) -> Result {
    let source_file = source_file.as_ref();
    let destination_file = destination_file.as_ref();
    println!("Will copy {} => {}", source_file.display(), destination_file.display());
    if let Some(parent) = destination_file.parent() {
        create_dir_if_missing(parent)?;
        if source_file.is_dir() {
            let mut options = fs_extra::dir::CopyOptions::new();
            options.overwrite = true;
            options.content_only = true;
            fs_extra::dir::copy(source_file, destination_file, &options)?;
        } else {
            std::fs::copy(source_file, destination_file)?;
        }
    } else {
        bail!("Cannot copy to the root path: {}", destination_file.display());
    }
    Ok(())
}

pub async fn mirror_directory(source: impl AsRef<Path>, destination: impl AsRef<Path>) -> Result {
    create_parent_dir_if_missing(destination.as_ref())?;
    if TARGET_OS == OS::Windows {
        crate::programs::robocopy::mirror_dir(source, destination).await
    } else {
        crate::programs::rsync::mirror_directory(source, destination).await
    }
}

pub fn expect_dir(path: impl AsRef<Path>) -> Result {
    path.as_ref()
        .is_dir()
        .then_some(())
        .context(anyhow!("{} is not a directory.", path.as_ref().display()))
}


pub fn expect_file(path: impl AsRef<Path>) -> Result {
    path.as_ref()
        .is_file()
        .then_some(())
        .context(anyhow!("{} is not a files.", path.as_ref().display()))
}

#[cfg(not(target_os = "windows"))]
#[context("Failed to update permissions on `{}`", path.as_ref().display())]
pub fn allow_owner_execute(path: impl AsRef<Path>) -> Result {
    use crate::anyhow::ResultExt;
    use std::os::unix::prelude::*;
    println!("Setting executable permission on {}", path.as_ref().display());
    let metadata = path.as_ref().metadata()?;
    let mut permissions = metadata.permissions();
    let mode = permissions.mode();
    let owner_can_execute = 0o0100;
    permissions.set_mode(mode | owner_can_execute);
    std::fs::set_permissions(path.as_ref(), permissions).anyhow_err()
}

#[cfg(target_os = "windows")]
#[context("Failed to update permissions on `{}`", path.as_ref().display())]
pub fn allow_owner_execute(path: impl AsRef<Path>) -> Result {
    // No-op on Windows.
    Ok(())
}
