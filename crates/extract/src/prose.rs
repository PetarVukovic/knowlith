//! Markdown and plain text.
//!
//! These are the only formats where the rendition is the file, so a block
//! offset is also an offset into the owner's own bytes. Nothing is rewritten,
//! reflowed or normalised here — the parser walks the text and records where
//! things are.

use knowlith_core::{Block, BlockKind};

/// Splits prose into blocks, with offsets into `text` itself.
pub fn blocks(text: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut section = Section::default();

    for (start, chunk) in paragraphs(text) {
        if let Some(level) = heading_level(chunk) {
            section.enter(level);
            let line = chunk.lines().next().unwrap_or(chunk).trim_end();
            out.push(block(
                section.heading_locator(),
                BlockKind::Heading,
                line,
                start,
            ));
            // A heading with body text on the very next line is one chunk;
            // the remainder still belongs to the section it just opened.
            let rest = chunk[line.len()..].trim_start_matches(['\n', '\r']);
            if !rest.trim().is_empty() {
                let offset = start + (chunk.len() - rest.len());
                out.push(block(
                    section.body_locator(),
                    BlockKind::Paragraph,
                    rest.trim_end(),
                    offset,
                ));
            }
        } else if is_list(chunk) {
            let mut at = start;
            for line in chunk.split_inclusive('\n') {
                let trimmed = line.trim_end_matches(['\n', '\r']);
                if !trimmed.trim().is_empty() {
                    out.push(block(
                        section.body_locator(),
                        BlockKind::ListItem,
                        trimmed,
                        at,
                    ));
                }
                at += line.len();
            }
        } else {
            out.push(block(
                section.body_locator(),
                BlockKind::Paragraph,
                chunk,
                start,
            ));
        }
    }

    out
}

fn block(locator: String, kind: BlockKind, text: &str, start: usize) -> Block {
    Block {
        locator,
        kind,
        text: text.to_string(),
        start_byte: start,
        end_byte: start + text.len(),
        page: None,
        sheet: None,
        row: None,
        cells: None,
    }
}

/// Where we are in the heading tree, so a block can be addressed as `§2.1 ¶3`
/// rather than by a running count the owner cannot map back to the document.
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

/// Yields each blank-line-separated chunk with its byte offset, trailing
/// whitespace trimmed off the end of the chunk but not off the offset.
fn paragraphs(text: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        // Skip blank lines.
        while i < bytes.len() && (bytes[i] == b'\n' || bytes[i] == b'\r') {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let start = i;
        // Run to the next blank line.
        loop {
            let Some(nl) = text[i..].find('\n') else {
                i = bytes.len();
                break;
            };
            i += nl + 1;
            let rest = &text[i..];
            if rest.is_empty() || rest.starts_with('\n') || rest.starts_with("\r\n") {
                break;
            }
        }
        let chunk = text[start..i].trim_end();
        if !chunk.is_empty() {
            out.push((start, chunk));
        }
    }

    out
}

fn heading_level(chunk: &str) -> Option<usize> {
    let hashes = chunk.bytes().take_while(|b| *b == b'#').count();
    if (1..=6).contains(&hashes) && chunk.as_bytes().get(hashes) == Some(&b' ') {
        Some(hashes)
    } else {
        None
    }
}

/// True when every non-empty line of the chunk is a list item. A single
/// numbered line inside a paragraph is not a list.
fn is_list(chunk: &str) -> bool {
    let mut any = false;
    for line in chunk.lines() {
        let t = line.trim_start();
        if t.is_empty() {
            continue;
        }
        any = true;
        let bullet = t.starts_with("- ") || t.starts_with("* ") || t.starts_with("+ ");
        let ordered = {
            let digits = t.bytes().take_while(u8::is_ascii_digit).count();
            digits > 0
                && matches!(t.as_bytes().get(digits), Some(b'.') | Some(b')'))
                && t.as_bytes().get(digits + 1) == Some(&b' ')
        };
        if !bullet && !ordered {
            return false;
        }
    }
    any
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_point_at_the_original_bytes() {
        let text = "# Uvjeti\n\nPopust je 5%.\n\n## Rokovi\n\nRok je 15 dana.\n";
        for b in blocks(text) {
            assert_eq!(&text[b.start_byte..b.end_byte], b.text);
        }
    }

    #[test]
    fn locators_follow_the_heading_tree() {
        let text = "# Uvjeti\n\nProdaja.\n\n## Popusti\n\nPet posto.\n\nDeset posto.\n";
        let got: Vec<_> = blocks(text).into_iter().map(|b| b.locator).collect();
        assert_eq!(got, ["§1", "§1 ¶1", "§1.1", "§1.1 ¶1", "§1.1 ¶2"]);
    }

    #[test]
    fn each_list_item_is_its_own_block() {
        let text = "- prvi\n- drugi\n- treći\n";
        let got = blocks(text);
        assert_eq!(got.len(), 3);
        assert!(got.iter().all(|b| b.kind == BlockKind::ListItem));
        assert_eq!(got[2].text, "- treći");
    }

    #[test]
    fn crlf_does_not_shift_offsets() {
        let text = "# Naslov\r\n\r\nPrva.\r\n\r\nDruga.\r\n";
        for b in blocks(text) {
            assert_eq!(&text[b.start_byte..b.end_byte], b.text);
        }
    }

    #[test]
    fn a_numbered_sentence_is_not_a_list() {
        let text = "Rok je 15 dana. 2026. je godina isporuke.";
        assert_eq!(blocks(text)[0].kind, BlockKind::Paragraph);
    }
}
