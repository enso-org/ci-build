use crate::prelude::*;
use mime::Mime;
use reqwest::header::HeaderMap;
use reqwest::header::HeaderValue;
use reqwest::header::CONTENT_TYPE;


pub trait ClientBuilderExt: Sized {
    fn default_content_type(self, mime_type: mime::Mime) -> Self;
}

impl ClientBuilderExt for reqwest::ClientBuilder {
    fn default_content_type(self, mime_type: Mime) -> Self {
        let mut header = HeaderMap::new();
        // We can safely unwrap, because we know that all mime types are in format that can be used
        // as HTTP header value.
        header.insert(CONTENT_TYPE, HeaderValue::from_str(mime_type.as_ref()).unwrap());
        self.default_headers(header)
    }
}
