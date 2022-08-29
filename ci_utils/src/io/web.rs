use crate::prelude::*;

use crate::fs::tokio::copy_to_file;
use anyhow::Context;
use reqwest::Client;
use reqwest::IntoUrl;
use reqwest::RequestBuilder;
use reqwest::Response;
use tokio::io::AsyncBufRead;

pub mod client;

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
pub async fn download_reader(url: impl IntoUrl) -> Result<impl AsyncBufRead + Unpin> {
    let response = reqwest::get(url).await?.error_for_status()?;
    Ok(async_reader(response))
}

/// Get the the response body as a byte stream.
pub async fn download_file(url: impl IntoUrl, output: impl AsRef<Path>) -> Result {
    stream_response_to_file(reqwest::get(url).await?, output).await
}


#[tracing::instrument(name="Streaming http response to a file.", skip(output, response), fields(
    dest = %output.as_ref().display(),
    url  = %response.url()
), err)]
pub async fn stream_response_to_file(response: Response, output: impl AsRef<Path>) -> Result {
    trace!("Streamed response: {:#?}", response);
    let response = handle_error_response(response).await?;
    let reader = async_reader(response);
    copy_to_file(reader, output).await?;
    Ok(())
}

pub fn async_reader(response: Response) -> impl AsyncBufRead + Unpin {
    tokio_util::io::StreamReader::new(response.bytes_stream().map_err(std::io::Error::other))
}

pub fn filename_from_content_disposition(value: &reqwest::header::HeaderValue) -> Result<&Path> {
    let regex = regex::Regex::new(r#"filename="?([^"]*)"?"#)?;
    let capture = regex
        .captures(value.to_str()?)
        .context("Field 'filename' not present in the header value.")?
        .get(1)
        .context("Missing capture group from regex.")?;
    Ok(&Path::new(capture.as_str()))
}

pub fn filename_from_response(response: &Response) -> Result<&Path> {
    use reqwest::header::CONTENT_DISPOSITION;
    let disposition = response
        .headers()
        .get(CONTENT_DISPOSITION)
        .context(format!("No {CONTENT_DISPOSITION} header present in the response."))?;
    filename_from_content_disposition(disposition)
}
#[cfg(test)]
mod tests {
    use super::*;

    use reqwest::header::HeaderValue;

    #[test]
    fn test_parsing_content_disposition() {
        let check_parse = |value: &'static str, expected: &str| {
            let header_value = HeaderValue::from_static(value);
            assert_eq!(
                filename_from_content_disposition(&header_value).unwrap(),
                Path::new(expected)
            );
        };
        let check_no_parse = |value: &'static str| {
            let header_value = HeaderValue::from_static(value);
            assert!(filename_from_content_disposition(&header_value).is_err());
        };

        check_parse(r#"attachment; filename="filename.jpg""#, "filename.jpg");
        check_parse(r#"form-data; name="fieldName"; filename="filename.jpg""#, "filename.jpg");
        check_parse(r#"attachment; filename=manifest.yaml"#, "manifest.yaml");
        check_no_parse(r#"attachment"#);
        check_no_parse(r#"inline"#);
    }
}
