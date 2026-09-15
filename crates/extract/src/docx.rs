//! Word documents.
//!
//! A `.docx` is a zip holding `word/document.xml`. Only two things are read
//! from it: the run text of each paragraph, and whether the paragraph's style
//! makes it a heading. Everything else Word stores — revisions, comments,
//! formatting, the author's name — is deliberately left behind.

use std::io::Read;

use knowlith_core::BlockKind;
use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};

use crate::builder::{Piece, Rendition};
use crate::error::ExtractError;

/// `word/document.xml` carries no XML declaration of its own in practice, so
/// the 1.0 rules apply.
const VERSION: XmlVersion = XmlVersion::Implicit1_0;

pub fn parse(bytes: &[u8]) -> Result<Rendition, ExtractError> {
    let xml = document_xml(bytes)?;

    let mut rendition = Rendition::new();
    let mut reader = Reader::from_str(&xml);
    let mut buf = Vec::new();

    let mut paragraph = String::new();
    let mut heading_level: Option<usize> = None;
    let mut in_text = false;
    let mut section = Section::default();

    loop {
        match reader.read_event_into(&mut buf) {
            Err(e) => return Err(ExtractError::Parse(e.to_string())),
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => match e.local_name().as_ref() {
                "t" => in_text = true,
                "p" => {
                    paragraph.clear();
                    heading_level = None;
                }
                _ => {}
            },
            Ok(Event::Empty(e)) => match e.local_name().as_ref() {
                "pStyle" => {
                    heading_level = e
                        .attributes()
                        .flatten()
                        .find(|a| a.key.local_name().as_ref() == "val")
                        .and_then(|a| a.normalized_value(VERSION).ok())
                        .as_deref()
                        .and_then(style_heading_level);
                }
                "tab" => paragraph.push('\t'),
                "br" => paragraph.push('\n'),
                _ => {}
            },
            Ok(Event::Text(t)) if in_text => {
                paragraph.push_str(&t.xml_content(VERSION));
            }
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                "t" => in_text = false,
                "p" => {
                    let text = paragraph.trim().to_string();
                    if !text.is_empty() {
                        match heading_level {
                            Some(level) => {
                                section.enter(level);
                                rendition.push(Piece::new(
                                    section.heading_locator(),
                                    BlockKind::Heading,
                                    text,
                                ));
                            }
                            None => rendition.push(Piece::new(
                                section.body_locator(),
                                BlockKind::Paragraph,
                                text,
                            )),
                        }
                    }
                    paragraph.clear();
                }
                _ => {}
            },
            _ => {}
        }
        buf.clear();
    }

    Ok(rendition)
}

fn document_xml(bytes: &[u8]) -> Result<String, ExtractError> {
    let cursor = std::io::Cursor::new(bytes.to_vec());
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| ExtractError::Parse(e.to_string()))?;
    let mut xml = String::new();
    archive
        .by_name("word/document.xml")
        .map_err(|_| ExtractError::Parse("no word/document.xml in archive".into()))?
        .read_to_string(&mut xml)
        .map_err(|e| ExtractError::Parse(e.to_string()))?;
    Ok(xml)
}

/// Mirrors the heading tree in [`crate::prose`], so a `.docx` and the `.md`
/// somebody exported from it address their paragraphs the same way.
#[derive(Default)]
struct Section {
    counters: [u32; 6],
    depth: usize,
    body: u32,
}

impl Section {
    fn enter(&mut self, level: usize) {
        self.counters[level - 1] += 1;
        for c in self.counters.iter_mut().skip(level) {
            *c = 0;
        }
        self.depth = level;
        self.body = 0;
    }

    fn number(&self) -> String {
        self.counters[..self.depth]
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(".")
    }

    fn heading_locator(&self) -> String {
        format!("§{}", self.number())
    }

    fn body_locator(&mut self) -> String {
        self.body += 1;
        if self.depth == 0 {
            format!("¶{}", self.body)
        } else {
            format!("§{} ¶{}", self.number(), self.body)
        }
    }
}

/// `Heading1`, `Heading 2`, `Naslov1` — Word localises style ids, so the digit
/// is the only reliable part.
fn style_heading_level(style: &str) -> Option<usize> {
    let lower = style.to_ascii_lowercase();
    if !(lower.starts_with("heading") || lower.starts_with("naslov")) {
        return None;
    }
    lower
        .chars()
        .find(|c| c.is_ascii_digit())
        .and_then(|c| c.to_digit(10))
        .map(|d| d as usize)
        .filter(|d| (1..=6).contains(d))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_styles_are_recognised_in_both_locales() {
        assert_eq!(style_heading_level("Heading1"), Some(1));
        assert_eq!(style_heading_level("Heading 3"), Some(3));
        assert_eq!(style_heading_level("Naslov2"), Some(2));
        assert_eq!(style_heading_level("BodyText"), None);
        assert_eq!(style_heading_level("Heading9"), None);
    }

    #[test]
    fn locators_follow_the_heading_tree() {
        let mut s = Section::default();
        s.enter(1);
        assert_eq!(s.heading_locator(), "§1");
        assert_eq!(s.body_locator(), "§1 ¶1");
        s.enter(2);
        assert_eq!(s.heading_locator(), "§1.1");
        assert_eq!(s.body_locator(), "§1.1 ¶1");
    }
}
