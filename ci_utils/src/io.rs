pub mod web;

use crate::prelude::*;

use anyhow::Context;
use reqwest::IntoUrl;
use std::time::Duration;
use tokio::io::AsyncRead;

use crate::archive::Format;
use crate::global::progress_bar;

/// Read the whole input and return its length.
///
/// Inputs content is discarded.
pub async fn read_length(mut read: impl AsyncRead + Unpin) -> Result<u64> {
    let mut sink = tokio::io::sink();
    tokio::io::copy(&mut read, &mut sink).anyhow_err().await
}

/// Get the the response body as a byte stream.
pub async fn download(url: impl IntoUrl) -> Result<impl Stream<Item = reqwest::Result<Bytes>>> {
    Ok(reqwest::get(url).await?.error_for_status()?.bytes_stream())
}

/// Get the full response body from URL as bytes.
pub async fn download_all(url: impl IntoUrl) -> anyhow::Result<Bytes> {
    let url = url.into_url()?;
    let bar = progress_bar(indicatif::ProgressBar::new_spinner);
    bar.enable_steady_tick(Duration::from_millis(100));
    bar.set_message(format!("Downloading {}", url));
    let response = reqwest::get(url).await?;
    if let Some(e) = response.error_for_status_ref().err() {
        let body = response.text().await?;
        Err(e).context(body)
    } else {
        response.bytes().await.map_err(Into::into)
    }
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

    debug!("Downloading {}", url_text);
    let contents = download_all(url).await?;
    let buffer = std::io::Cursor::new(contents);

    debug!("Extracting {} to {}", filename.display(), output_dir.as_ref().display());
    let format = Format::from_filename(filename)?;
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

    debug!("Will download {} => {}", url_to_get, output_path.display());
    let response = client.get(url_to_get).send().await?.error_for_status()?;

    if let Some(parent_dir) = output_path.parent() {
        crate::fs::create_dir_if_missing(parent_dir)?;
    }
    let output = tokio::fs::OpenOptions::new().write(true).create(true).open(&output_path).await?;
    response
        .bytes_stream()
        .map_err(anyhow::Error::from)
        // We must use fold (rather than foreach) to properly keep `output` alive long enough.
        .try_fold(output, |mut output, chunk| async move {
            let _amount = output.write(&chunk.clone()).await?;
            Ok(output)
        })
        .await?;
    debug!("Download finished: {}", output_path.display());
    Ok(output_path)
}

// pub async fn stream_to_file<E: Into<Box<dyn std::error::Error + Send + Sync>>>(
//     stream: impl Stream<Item = std::result::Result<Bytes, E>> + Unpin,
//     output_path: impl AsRef<Path>,
// ) -> Result {
//     let mut reader = tokio_util::io::StreamReader::new(stream.map_err(std::io::Error::other));
//     let mut output = crate::fs::tokio::create(output_path).await?;
//     tokio::io::copy(&mut reader, &mut output).await?;
//     Ok(())
// }


#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::copy;
    use crate::fs::create_parent_dir_if_missing;
    use crate::fs::mirror_directory;
    use tempfile::tempdir;

    #[tokio::test]
    #[ignore]
    async fn test_download() -> Result {
        debug!("Hello world!");
        let url = "https://speed.hetzner.de/100MB.bin";
        download_all(url).await?;
        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn copy_dir_with_symlink() -> Result {
        let dir = tempdir()?;
        let foo = dir.join_iter(["src", "foo.txt"]);
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
