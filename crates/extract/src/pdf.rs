//! PDFs.
//!
//! A PDF has no paragraphs — it has glyphs at coordinates. What comes back
//! from text extraction is a reconstruction, which is why a PDF block is
//! addressed as `page 4, ¶2`: the page is a fact the file records, the
//! paragraph is our reading of it.
//!
//! Scanned PDFs yield no text at all. That is reported as a skip with a reason
//! the owner can act on, never as an empty document that silently contributes
//! nothing.

use knowlith_core::BlockKind;

use crate::builder::{Piece, Rendition};
use crate::error::ExtractError;

pub fn parse(bytes: &[u8]) -> Result<Rendition, ExtractError> {
    let pages = pdf_extract::extract_text_from_mem_by_pages(bytes)
        .map_err(|e| ExtractError::Parse(e.to_string()))?;

    let mut rendition = Rendition::new();
    for (index, page) in pages.iter().enumerate() {
        let page_number = index as u32 + 1;
        for (ordinal, chunk) in split_paragraphs(page).into_iter().enumerate() {
            rendition.push(
                Piece::new(
                    format!("page {page_number}, ¶{}", ordinal + 1),
                    BlockKind::Paragraph,
                    chunk,
                )
                .page(page_number),
            );
        }
    }

    if rendition.is_empty() {
        return Err(ExtractError::NoText);
    }
    Ok(rendition)
}

/// Groups extracted lines into paragraphs on blank lines, and joins the lines
/// within a paragraph with a space.
///
/// Extractors break a visual line wherever the glyph run ends, so a sentence
/// arrives split across several lines. Keeping those breaks would mean a quote
/// of one sentence never matches the stored text.
fn split_paragraphs(page: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current: Vec<&str> = Vec::new();

    for line in page.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !current.is_empty() {
                out.push(current.join(" "));
                current.clear();
            }
        } else {
            current.push(trimmed);
        }
    }
    if !current.is_empty() {
        out.push(current.join(" "));
    }

    out.retain(|p| !p.trim().is_empty());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapped_lines_rejoin_into_one_paragraph() {
        let page = "Ponuda vrijedi\n14 dana od izdavanja.\n\nPlaćanje u roku\n15 dana.\n";
        assert_eq!(
            split_paragraphs(page),
            [
                "Ponuda vrijedi 14 dana od izdavanja.",
                "Plaćanje u roku 15 dana."
            ]
        );
    }

    #[test]
    fn a_page_of_whitespace_yields_nothing() {
        assert!(split_paragraphs("   \n\n  \n").is_empty());
    }
}
