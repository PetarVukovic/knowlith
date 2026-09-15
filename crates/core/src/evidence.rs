//! The mechanical half of the evidence gate.
//!
//! A model proposes a claim and says which sentence it came from. Before that
//! claim is allowed anywhere near the Lake, this module checks the citation the
//! way a compiler checks a type: the span must exist, and the bytes it covers
//! must be the quote. No model is consulted, nothing is scored, and the answer
//! is the same on every machine.
//!
//! A second model agreeing that a claim "looks well supported" is not this.
//! That is another opinion; this is arithmetic.

use serde::{Deserialize, Serialize};

use crate::document::Document;

/// A claim's citation: one span of one document, quoted verbatim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    pub document_id: String,
    /// The human-readable address shown to the owner.
    pub locator: String,
    pub start_byte: usize,
    pub end_byte: usize,
    /// What the model says is written there.
    pub quote: String,
}

/// Why a citation was refused. Each variant is a distinct bug in the caller,
/// so they are kept apart rather than collapsed into one "invalid".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum Rejection {
    /// The cited document is not in the Lake.
    UnknownDocument { document_id: String },
    /// `start_byte >= end_byte`, or the span runs past the end of the text.
    SpanOutOfRange {
        start_byte: usize,
        end_byte: usize,
        text_len: usize,
    },
    /// The span exists but does not begin and end on a character boundary —
    /// the model counted characters, or a parser version drifted.
    SpanNotOnCharBoundary { start_byte: usize, end_byte: usize },
    /// The span exists and the bytes are something else. This is the case that
    /// matters: a plausible sentence that the document does not contain.
    QuoteMismatch { found: String, claimed: String },
}

/// Checks a citation against the document it points at.
///
/// Whitespace is normalised on both sides before comparison — a model that
/// re-wraps a line has still quoted the sentence, and refusing that would
/// reject true claims for a reason the owner cannot act on. Nothing else is
/// forgiven: no case folding, no punctuation stripping, no fuzzy distance.
pub fn verify(document: &Document, evidence: &Evidence) -> Result<(), Rejection> {
    if document.id != evidence.document_id {
        return Err(Rejection::UnknownDocument {
            document_id: evidence.document_id.clone(),
        });
    }

    let text = &document.text;
    if evidence.start_byte >= evidence.end_byte || evidence.end_byte > text.len() {
        return Err(Rejection::SpanOutOfRange {
            start_byte: evidence.start_byte,
            end_byte: evidence.end_byte,
            text_len: text.len(),
        });
    }

    if !text.is_char_boundary(evidence.start_byte) || !text.is_char_boundary(evidence.end_byte) {
        return Err(Rejection::SpanNotOnCharBoundary {
            start_byte: evidence.start_byte,
            end_byte: evidence.end_byte,
        });
    }

    let found = &text[evidence.start_byte..evidence.end_byte];
    if normalise(found) == normalise(&evidence.quote) {
        Ok(())
    } else {
        Err(Rejection::QuoteMismatch {
            found: found.to_string(),
            claimed: evidence.quote.clone(),
        })
    }
}

/// Collapses every run of whitespace to a single space and trims the ends.
fn normalise(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Finds the span of `quote` in the document, when the model gave the text but
/// not the offsets.
///
/// Whitespace is treated the same way [`verify`] treats it, and that
/// agreement is not cosmetic. A document wraps a sentence across two lines;
/// a model copies it back as one. Matching byte-for-byte here while
/// forgiving the difference in `verify` meant the compiler threw away claims
/// that the storage layer would have happily accepted — on a real folder
/// that was most of what it dropped.
///
/// Returns `None` when the quote does not occur, and also when it occurs more
/// than once: an ambiguous citation is not a citation, and guessing the first
/// match would silently attach the claim to the wrong paragraph.
pub fn locate(document: &Document, quote: &str) -> Option<(usize, usize)> {
    let needle = normalise(quote);
    if needle.is_empty() {
        return None;
    }

    // The document with runs of whitespace collapsed, plus the byte offset in
    // the original that each normalised byte came from. Searching the
    // normalised text and mapping back is what makes a re-wrapped quote find
    // its real span rather than no span at all.
    let (flat, offsets) = flatten(&document.text);

    let mut found: Option<(usize, usize)> = None;
    let mut from = 0usize;
    while let Some(rel) = flat[from..].find(&needle) {
        let start = from + rel;
        let end = start + needle.len();
        if found.is_some() {
            return None;
        }
        // `offsets` has one more entry than `flat` is long, so the end of the
        // last match maps too.
        found = Some((offsets[start], offsets[end]));
        from = end;
    }
    found
}

/// Collapses whitespace and remembers where every byte came from.
///
/// `offsets[i]` is the byte in the original text that `flat[i]` starts at,
/// and `offsets[flat.len()]` is where the match should end.
fn flatten(text: &str) -> (String, Vec<usize>) {
    let mut flat = String::with_capacity(text.len());
    let mut offsets = Vec::with_capacity(text.len() + 1);
    let mut pending_space: Option<usize> = None;

    for (offset, c) in text.char_indices() {
        if c.is_whitespace() {
            // A run of whitespace becomes at most one space, and only once
            // something has been written — leading whitespace is dropped.
            // The space maps to where the run *starts*, so a match that ends
            // just before it does not swallow it.
            if !flat.is_empty() && pending_space.is_none() {
                pending_space = Some(offset);
            }
            continue;
        }
        if let Some(space_at) = pending_space.take() {
            offsets.push(space_at);
            flat.push(' ');
        }
        for _ in 0..c.len_utf8() {
            offsets.push(offset);
        }
        flat.push(c);
    }

    offsets.push(text.len());
    (flat, offsets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Block, BlockKind, DocumentKind};

    fn doc(text: &str) -> Document {
        Document {
            id: "doc:test".into(),
            path: "/tmp/test.md".into(),
            name: "test.md".into(),
            kind: DocumentKind::Markdown,
            byte_len: text.len() as u64,
            sha256: "0".repeat(64),
            text: text.to_string(),
            text_sha256: "0".repeat(64),
            verbatim: true,
            modified: "2026-01-01T00:00:00Z".into(),
            columns: None,
            blocks: vec![Block {
                locator: "¶1".into(),
                kind: BlockKind::Paragraph,
                text: text.to_string(),
                start_byte: 0,
                end_byte: text.len(),
                page: None,
                sheet: None,
                row: None,
                cells: None,
            }],
        }
    }

    fn ev(start: usize, end: usize, quote: &str) -> Evidence {
        Evidence {
            document_id: "doc:test".into(),
            locator: "¶1".into(),
            start_byte: start,
            end_byte: end,
            quote: quote.into(),
        }
    }

    #[test]
    fn exact_span_passes() {
        let d = doc("Popust je 5% za stalne kupce.");
        assert_eq!(verify(&d, &ev(0, 13, "Popust je 5%")), Ok(()));
    }

    #[test]
    fn rewrapped_quote_still_passes() {
        let d = doc("Rok\nplaćanja je\n15 dana.");
        assert_eq!(verify(&d, &ev(0, 25, "Rok plaćanja je 15 dana.")), Ok(()));
    }

    #[test]
    fn plausible_but_absent_quote_is_refused() {
        let d = doc("Popust je 5% za stalne kupce.");
        let Err(Rejection::QuoteMismatch { claimed, .. }) = verify(&d, &ev(0, 12, "Popust je 8%"))
        else {
            panic!("a quote the document does not contain must be refused");
        };
        assert_eq!(claimed, "Popust je 8%");
    }

    #[test]
    fn span_past_the_end_is_refused() {
        let d = doc("kratko");
        assert!(matches!(
            verify(&d, &ev(0, 999, "kratko")),
            Err(Rejection::SpanOutOfRange { .. })
        ));
    }

    #[test]
    fn span_splitting_a_multibyte_character_is_refused() {
        // "plaćanja" — the ć occupies bytes 3 and 4, so byte 4 lands inside it.
        let d = doc("plaćanja");
        assert!(matches!(
            verify(&d, &ev(0, 4, "plać")),
            Err(Rejection::SpanNotOnCharBoundary { .. })
        ));
    }

    #[test]
    fn empty_span_is_refused() {
        let d = doc("bilo što");
        assert!(matches!(
            verify(&d, &ev(3, 3, "")),
            Err(Rejection::SpanOutOfRange { .. })
        ));
    }

    #[test]
    fn locate_finds_a_unique_quote() {
        let d = doc("Ponuda vrijedi 14 dana od izdavanja.");
        assert_eq!(locate(&d, "14 dana"), Some((15, 22)));
    }

    #[test]
    fn locate_finds_a_sentence_the_document_wrapped_across_lines() {
        let d = doc("Razlog je manjak\nservisera u sijecnju i veljaci.");
        let (start, end) = locate(&d, "Razlog je manjak servisera u sijecnju i veljaci.")
            .expect("a re-wrapped quote is still the same sentence");
        assert_eq!(start, 0);
        assert_eq!(end, d.text.len());
        // And what it points at still passes the gate.
        assert_eq!(
            verify(&d, &ev(start, end, "Razlog je manjak servisera u sijecnju i veljaci.")),
            Ok(())
        );
    }

    #[test]
    fn locate_maps_back_through_multibyte_characters() {
        let d = doc("Rok\n  plaćanja   je\n15 dana.");
        let (start, end) = locate(&d, "plaćanja je 15 dana.").unwrap();
        assert!(d.text.is_char_boundary(start) && d.text.is_char_boundary(end));
        assert_eq!(&d.text[start..end], "plaćanja   je\n15 dana.");
    }

    #[test]
    fn locate_still_refuses_a_quote_that_is_merely_similar() {
        let d = doc("Popust od 5% odobrava se stalnim kupcima.");
        assert_eq!(locate(&d, "Popust od 8% odobrava se stalnim kupcima."), None);
    }

    #[test]
    fn locate_refuses_an_ambiguous_quote() {
        let d = doc("15 dana. Rok je 15 dana.");
        assert_eq!(locate(&d, "15 dana"), None);
    }
}
