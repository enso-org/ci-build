use crate::prelude::*;

use crate::fs::tokio::copy_to_file;
use anyhow::Context;
use reqwest::Client;
use reqwest::IntoUrl;
use reqwest::RequestBuilder;
use reqwest::Response;
use tokio::io::AsyncBufRead;

pub async fn handle_error_response(response: Response) -> Result<Response> {
    if let Some(e) = response.error_for_status_ref().err() {
        let e = Err(e);
        return match response.text().await {
            Ok(body) => e.context(format!("Error message body: {body}")),
            Err(body_error) =>
                e.context(format!("Failed to get error response body: {body_error}")),
        };
    } else {
        Ok(response)
    }
}

pub async fn get(client: &Client, url: impl IntoUrl) -> Result<Response> {
    execute(client.get(url)).await
}

pub async fn execute(request_builder: RequestBuilder) -> Result<Response> {
    handle_error_response(request_builder.send().await?).await
}

/// Get the the response body as a byte stream.
pub async fn download_stream(
    url: impl IntoUrl,
) -> Result<impl Stream<Item = reqwest::Result<Bytes>>> {
    Ok(handle_error_response(reqwest::get(url).await?).await?.bytes_stream())
}

/// Get the the response body as a byte stream.
pub async fn download_file(url: impl IntoUrl, output: impl AsRef<Path>) -> Result {
    stream_response_to_file(reqwest::get(url).await?, output).await
}


#[tracing::instrument(name="Streaming http response to a file.", skip(output), fields(dest=%output.as_ref().display()), err)]
pub async fn stream_response_to_file(response: Response, output: impl AsRef<Path>) -> Result {
    let response = handle_error_response(response).await?;
    let reader = async_reader(response);
    copy_to_file(reader, output).await?;
    Ok(())
}

pub fn async_reader(response: Response) -> impl AsyncBufRead + Unpin {
    tokio_util::io::StreamReader::new(response.bytes_stream().map_err(std::io::Error::other))
}
