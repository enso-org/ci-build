use crate::prelude::*;
use pulldown_cmark::Event;
use pulldown_cmark::HeadingLevel;
use pulldown_cmark::Options;
use pulldown_cmark::Parser;
use pulldown_cmark::Tag::Heading;
use std::ops::Range;

#[derive(Clone, Debug)]
pub struct Entry {
    pub header:   String,
    pub contents: String,
}

pub struct Header<'a> {
    /// Text of the header.
    pub text: &'a str,
    /// Position in the changelog file text.
    pub pos:  Range<usize>,
}

impl<'a> Header<'a> {
    pub fn new(whole_text: &'a str, event: Event, position: Range<usize>) -> Option<Self> {
        is_release_notes_header(&event)
            .then_some(Self { text: whole_text[position.clone()].trim(), pos: position })
    }
}

pub fn is_release_notes_header(event: &Event) -> bool {
    matches!(event, Event::Start(Heading(HeadingLevel::H1, _, _)))
}

pub fn iterate_headers<'a>(changelog_text: &'a str) -> impl Iterator<Item = Header<'a>> + 'a {
    Parser::new_ext(changelog_text, Options::all())
        .into_offset_iter()
        .filter_map(|(e, pos)| Header::new(changelog_text, e, pos))
}

pub fn retrieve_unreleased_release_notes(changelog: impl AsRef<Path>) -> Result<Entry> {
    let text = std::fs::read_to_string(changelog)?;
    let mut headers = iterate_headers(&text);
    let first_header =
        headers.next().ok_or_else(|| anyhow!("Failed to find first level one header."))?;
    let next_header_start = headers.next().map_or(text.len() + 1, |h| h.pos.start);
    let contents = text[first_header.pos.end..next_header_start].trim();
    let entry = Entry { header: first_header.text.to_string(), contents: contents.to_string() };
    Ok(entry)
}

pub fn retrieve_last_release(changelog: impl AsRef<Path>) -> Result<Version> {
    let text = std::fs::read_to_string(changelog.as_ref())?;
    let ret = iterate_headers(&text)
        .find_map(|header| ide_ci::program::version::find_in_text(header.text).ok())
        .ok_or_else(|| {
            anyhow!(
                "No release header with version number found in the {}",
                changelog.as_ref().display()
            )
        });
    ret
}

#[test]
fn aaaa() -> Result {
    let opts = pulldown_cmark::Options::all();

    let path = r"H:\NBO\enso\app\gui\CHANGELOG.md";
    let text = std::fs::read_to_string(path)?;
    let entry = retrieve_unreleased_release_notes(path)?;
    dbg!(entry);
    Ok(())
}
