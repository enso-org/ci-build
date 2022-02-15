use crate::prelude::*;
use anyhow::Context;
use fs_extra::dir::CopyOptions;
use platforms::TARGET_OS;

use crate::archive::Format;
use reqwest::IntoUrl;

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
        bail!("No parent directory for path {}", path.as_ref().display())
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

/// Get the full response body from URL as bytes.
pub async fn download(url: impl IntoUrl) -> anyhow::Result<Bytes> {
    reqwest::get(url).await?.bytes().await.map_err(Into::into)
}

/// Take the trailing filename from URL path.
///
/// ```
/// use std::path::PathBuf;
/// use url::Url;
/// use ide_ci::io::filename_from_url;
/// let url = Url::parse("https://github.com/enso-org/ide/releases/download/v2.0.0-alpha.18/enso-win-2.0.0-alpha.18.exe").unwrap();
/// assert_eq!(filename_from_url(&url).unwrap(), PathBuf::from("enso-win-2.0.0-alpha.18.exe"));
/// ```
pub fn filename_from_url(url: &Url) -> anyhow::Result<PathBuf> {
    url.path_segments()
        .ok_or_else(|| anyhow!("Cannot split URL '{}' into path segments!", url))?
        .last()
        .ok_or_else(|| anyhow!("No segments in path for URL '{}'", url))
        .map(PathBuf::from)
        .map_err(Into::into)
}

/// Downloads archive from URL and extracts it into an output path.
pub async fn download_and_extract(
    url: impl IntoUrl,
    output_dir: impl AsRef<Path>,
) -> anyhow::Result<()> {
    let url = url.into_url()?;
    let url_text = url.to_string();
    let filename = filename_from_url(&url)?;

    println!("Downloading {}", url_text);
    let contents = download(url).await?;
    let buffer = std::io::Cursor::new(contents);

    println!("Extracting {} to {}", filename.display(), output_dir.as_ref().display());
    let format = Format::from_filename(&PathBuf::from(filename))?;
    format.extract(buffer, output_dir.as_ref()).with_context(|| {
        format!("Failed to extract data from {} to {}.", url_text, output_dir.as_ref().display(),)
    })
}

/// Download file at base_url/subpath to output_dir_base/subpath.
pub async fn download_relative(
    client: &reqwest::Client,
    base_url: &Url,
    output_dir_base: impl AsRef<Path>,
    subpath: &Path,
) -> Result<PathBuf> {
    let url_to_get = base_url.join(&subpath.display().to_string())?;
    let output_path = output_dir_base.as_ref().join(subpath);

    println!("Will download {} => {}", url_to_get, output_path.display());
    let response = client.get(url_to_get).send().await?.error_for_status()?;

    if let Some(parent_dir) = output_path.parent() {
        create_dir_if_missing(parent_dir)?;
    }
    let output = tokio::fs::OpenOptions::new().write(true).create(true).open(&output_path).await?;
    response
        .bytes_stream()
        .map_err(anyhow::Error::from)
        // We must use fold (rather than foreach) to properly keep `output` alive long enough.
        .try_fold(output, |mut output, chunk| async move {
            output.write(&chunk.clone()).await?;
            Ok(output)
        })
        .await?;
    println!("Download finished: {}", output_path.display());
    Ok(output_path)
}

pub fn copy_to(source_file: impl AsRef<Path>, dest_dir: impl AsRef<Path>) -> Result {
    create_dir_if_missing(dest_dir.as_ref())?;
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
            options.copy_inside = true;
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
    path.as_ref().is_dir().then_some(()).context(anyhow!("{} is not a directory.", path.as_ref().display()))
}


pub fn expect_file(path: impl AsRef<Path>) -> Result {
    path.as_ref().is_file().then_some(()).context(anyhow!("{} is not a files.", path.as_ref().display()))
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


#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    #[ignore]
    async fn copy_dir_with_symlink() -> Result {
        let dir = tempdir()?;
        let foo = dir.join_many(["src", "foo.txt"]);
        std::env::set_current_dir(&dir)?;
        create_parent_dir_if_missing(&foo)?;
        std::fs::write(&foo, "foo")?;

        let bar = foo.with_file_name("bar");

        // Command::new("ls").arg("-laR").run_ok().await?;
        #[cfg(not(target_os = "windows"))]
        std::os::unix::fs::symlink(foo.file_name().unwrap(), &bar)?;
        #[cfg(target_os = "windows")]
        std::os::windows::fs::symlink_file(foo.file_name().unwrap(), &bar)?;

        copy(foo.parent().unwrap(), foo.parent().unwrap().with_file_name("dest"))?;

        mirror_directory(foo.parent().unwrap(), foo.parent().unwrap().with_file_name("dest2"))
            .await?;

        tokio::process::Command::new(r"C:\msys64\usr\bin\ls.exe")
            .arg("-laR")
            .status()
            .await?
            .exit_ok()?;

        Ok(())
    }
}
