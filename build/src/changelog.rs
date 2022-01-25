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

pub fn is_release_notes_header(event: &Event) -> bool {
    matches!(event, Event::Start(Heading(HeadingLevel::H1, _, _)))
}

pub fn iterate_headers_text<'a>(changelog_text: &'a str) -> impl Iterator<Item = &'a str> + 'a {
    Parser::new_ext(changelog_text, Options::all())
        .into_offset_iter()
        .filter_map(|(e, pos)| is_release_notes_header(&e).then_some(&changelog_text[pos]))
}

pub fn iterate_headers_pos<'a>(changelog_text: &'a str) -> impl Iterator<Item = Range<usize>> + 'a {
    Parser::new_ext(changelog_text, Options::all())
        .into_offset_iter()
        .filter_map(|(e, pos)| is_release_notes_header(&e).then_some(pos))
}

pub fn retrieve_unreleased_release_notes(changelog: impl AsRef<Path>) -> Result<Entry> {
    let text = std::fs::read_to_string(changelog)?;
    let mut headers = iterate_headers_pos(&text);
    let first_header_pos =
        headers.next().ok_or_else(|| anyhow!("Failed to find first level one header."))?;
    let next_header_start = headers.next().map_or(text.len() + 1, |pos| pos.start);
    let header = text[first_header_pos.clone()].trim();
    let contents = text[first_header_pos.end..next_header_start].trim();
    let entry = Entry { header: header.to_string(), contents: contents.to_string() };
    Ok(entry)
}

pub fn retrieve_last_release(changelog: impl AsRef<Path>) -> Result<Version> {
    let text = std::fs::read_to_string(changelog.as_ref())?;
    let ret = iterate_headers_pos(&text)
        .find_map(|entry| ide_ci::program::version::find_in_text(&text[entry]).ok())
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
