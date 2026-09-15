//! Assembling a rendition and its blocks together, so offsets cannot drift.
//!
//! Every parser that has to *build* its text — PDF, DOCX, spreadsheets — goes
//! through this. Text and offsets are produced in one pass, which removes the
//! whole class of bug where a parser writes the rendition and then searches it
//! for its own blocks.

use knowlith_core::{Block, BlockKind};

/// Separator between blocks in a built rendition. Two newlines, so the text
/// reads as a document rather than a dump.
const GAP: &str = "\n\n";

#[derive(Debug, Default)]
pub struct Rendition {
    text: String,
    blocks: Vec<Block>,
}

/// Everything about a block except where it landed.
pub struct Piece {
    pub locator: String,
    pub kind: BlockKind,
    pub text: String,
    pub page: Option<u32>,
    pub sheet: Option<String>,
    pub row: Option<u32>,
    pub cells: Option<Vec<String>>,
}

impl Piece {
    pub fn new(locator: impl Into<String>, kind: BlockKind, text: impl Into<String>) -> Self {
        Self {
            locator: locator.into(),
            kind,
            text: text.into(),
            page: None,
            sheet: None,
            row: None,
            cells: None,
        }
    }

    pub fn page(mut self, page: u32) -> Self {
        self.page = Some(page);
        self
    }

    pub fn row(mut self, sheet: Option<&str>, row: u32, cells: Vec<String>) -> Self {
        self.sheet = sheet.map(str::to_string);
        self.row = Some(row);
        self.cells = Some(cells);
        self
    }
}

impl Rendition {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a block and records the span it occupies. Empty text is skipped
    /// rather than stored: a zero-width block can never be cited, and it would
    /// make `block_at` ambiguous at its own offset.
    pub fn push(&mut self, piece: Piece) {
        if piece.text.trim().is_empty() {
            return;
        }
        if !self.text.is_empty() {
            self.text.push_str(GAP);
        }
        let start_byte = self.text.len();
        self.text.push_str(&piece.text);
        self.blocks.push(Block {
            locator: piece.locator,
            kind: piece.kind,
            text: piece.text,
            start_byte,
            end_byte: self.text.len(),
            page: piece.page,
            sheet: piece.sheet,
            row: piece.row,
            cells: piece.cells,
        });
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    pub fn finish(self) -> (String, Vec<Block>) {
        (self.text, self.blocks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_address_their_own_text() {
        let mut r = Rendition::new();
        r.push(Piece::new("¶1", BlockKind::Paragraph, "Popust je 5%."));
        r.push(Piece::new("¶2", BlockKind::Paragraph, "Rok je 15 dana."));
        let (text, blocks) = r.finish();
        for b in &blocks {
            assert_eq!(&text[b.start_byte..b.end_byte], b.text);
        }
        assert_eq!(text, "Popust je 5%.\n\nRok je 15 dana.");
    }

    #[test]
    fn blank_pieces_are_dropped() {
        let mut r = Rendition::new();
        r.push(Piece::new("¶1", BlockKind::Paragraph, "   \n  "));
        assert!(r.is_empty());
    }
}
