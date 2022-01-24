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

pub fn next_header_pos<'a>(
    iter: &mut impl Iterator<Item = (Event<'a>, Range<usize>)>,
) -> Option<Range<usize>> {
    iter.find_map(|(e, pos)| is_release_notes_header(&e).then_some(pos))
}

pub fn retrieve_latest_release_notes(changelog: impl AsRef<Path>) -> Result<Entry> {
    let text = std::fs::read_to_string(changelog)?;
    let mut parser = Parser::new_ext(&text, Options::all()).into_offset_iter();

    let first_header_pos = next_header_pos(&mut parser)
        .ok_or_else(|| anyhow!("Failed to find first level one header."))?;
    let next_header_start = next_header_pos(&mut parser).map_or(text.len() + 1, |pos| pos.start);

    let header = text[first_header_pos.clone()].trim();
    let contents = text[first_header_pos.end..next_header_start].trim();
    let entry = Entry { header: header.to_string(), contents: contents.to_string() };
    Ok(entry)
}

#[test]
fn aaaa() -> Result {
    let opts = pulldown_cmark::Options::all();

    let path = r"H:\NBO\enso\app\gui\CHANGELOG.md";
    let text = std::fs::read_to_string(path)?;
    let mut parser = Parser::new_ext(&text, opts).into_offset_iter();

    let mut next_header_pos =
        || parser.find(|(e, _)| is_release_notes_header(e)).map(|(_, pos)| pos);

    let first_header_pos =
        next_header_pos().ok_or_else(|| anyhow!("Failed to find first level one header."))?;
    let next_header_start = next_header_pos().map_or(text.len() + 1, |pos| pos.start);

    let header = text[first_header_pos.clone()].trim();
    let contents = text[first_header_pos.end..next_header_start].trim();
    let entry = Entry { header: header.to_string(), contents: contents.to_string() };

    dbg!(entry);
    Ok(())
}
