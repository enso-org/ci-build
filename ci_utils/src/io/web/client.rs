use crate::prelude::*;

use crate::archive::Format;
use crate::global::progress_bar;
use reqwest::Client;
use reqwest::IntoUrl;
use std::time::Duration;


/// Get the the response body as a byte stream.
pub async fn download(
    client: &Client,
    url: impl IntoUrl,
) -> Result<impl Stream<Item = reqwest::Result<Bytes>>> {
    Ok(client.get(url).send().await?.error_for_status()?.bytes_stream())
}

/// Get the full response body from URL as bytes.
pub async fn download_all(client: &Client, url: impl IntoUrl) -> anyhow::Result<Bytes> {
    let url = url.into_url()?;
    let bar = progress_bar(indicatif::ProgressBar::new_spinner);
    bar.enable_steady_tick(Duration::from_millis(100));
    bar.set_message(format!("Downloading {}", url));
    let response = client.get(url).send().await?;
    if let Some(e) = response.error_for_status_ref().err() {
        let body = response.text().await?;
        Err(e).context(body)
    } else {
        response.bytes().await.map_err(Into::into)
    }
}

/// Downloads archive from URL and extracts it into an output path.
pub async fn download_and_extract(
    client: &Client,
    url: impl IntoUrl,
    output_dir: impl AsRef<Path>,
) -> anyhow::Result<()> {
    let url = url.into_url()?;
    let url_text = url.to_string();
    let filename = crate::io::filename_from_url(&url)?;
    let format = Format::from_filename(&filename)?;

    debug!("Downloading {}", url_text);
    // FIXME: dont keep the whole download in the memory.
    let contents = download_all(client, url).await?;
    let buffer = std::io::Cursor::new(contents);

    debug!("Extracting {} to {}", filename.display(), output_dir.as_ref().display());
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
            output.write(&chunk.clone()).await?;
            Ok(output)
        })
        .await?;
    debug!("Download finished: {}", output_path.display());
    Ok(output_path)
}
