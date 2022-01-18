use crate::prelude::*;
use fs_extra::dir::CopyOptions;

use crate::archive::ArchiveFormat;
use reqwest::IntoUrl;
use snafu::ResultExt;

#[derive(Debug, Snafu)]
pub enum IoOperationFailed {
    #[snafu(display("Failed to create directory {}: {}", path.display(), source))]
    CreateDir { path: PathBuf, source: std::io::Error },
    #[snafu(display("Failed to remove {}: {}", path.display(), source))]
    Remove { path: PathBuf, source: std::io::Error },
}

/// Create a directory (and all missing parent directories),
///
/// Does not fail when a directory already exists.
pub fn create_dir_if_missing(path: impl AsRef<Path>) -> std::io::Result<()> {
    let result = std::fs::create_dir_all(path);
    match result {
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        result => result,
    }
}

/// Remove a directory with all its subtree.
///
/// Does not fail if the directory is already gone.
pub fn remove_dir_if_exists(path: impl AsRef<Path>) -> std::io::Result<()> {
    let result = std::fs::remove_dir_all(path);
    match result {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        result => dbg!(result),
    }
}

/// Remove a regular file.
///
/// Does not fail if the file is already gone.
pub fn remove_file_if_exists(path: impl AsRef<Path>) -> std::io::Result<()> {
    let result = std::fs::remove_file(path);
    match result {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        result => dbg!(result),
    }
}


pub fn remove_if_exists(path: impl AsRef<Path>) -> std::io::Result<()> {
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
    remove_dir_if_exists(&path).context(Remove { path: path.clone() })?;
    create_dir_if_missing(&path).context(CreateDir { path: path.clone() })?;
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
    let filename = filename_from_url(&url)?;

    println!("Downloading {}", url);
    let contents = download(url).await?;
    let buffer = std::io::Cursor::new(contents);

    println!("Extracting {} to {}", filename.display(), output_dir.as_ref().display());
    let format = ArchiveFormat::from_filename(&PathBuf::from(filename))?;
    format.extract(buffer, output_dir)
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
