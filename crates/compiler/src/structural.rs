//! Stage 1: deciding what is worth asking about.
//!
//! Every document sent to a model costs money, time and a share of somebody's
//! rate limit, and most of a company's folder is not knowledge. This stage
//! runs on names and structure alone — no model, no network — and its job is
//! to throw away the obvious before anything expensive happens.
//!
//! It is deliberately conservative. Sending one template too many wastes a
//! few cents; skipping the one document that holds the payment terms means
//! the product silently does not know them.

use knowlith_core::{Document, DocumentKind};

/// Filenames that are a form to fill in rather than a statement of anything.
///
/// A template's sentences read exactly like rules — "Rok plaćanja je 15
/// dana." sits in the blank offer document too — so without this the
/// compiler learns the company's own boilerplate back from itself and counts
/// it as independent agreement.
const TEMPLATE_WORDS: [&str; 6] = ["predlozak", "predložak", "template", "obrazac", "sablona", "šablona"];

/// Documents whose content is personal rather than operational.
///
/// These are held back rather than read. A payroll sheet answers no question
/// an AI tool should be answering, and reading it would put salaries and
/// national ID numbers into a context store the whole company queries.
const PERSONAL_WORDS: [&str; 8] = ["place", "plaće", "placa", "plaća", "payroll", "zaposlenici", "ugovor-o-radu", "bolovanje"];

/// Last year's copy is deliberately **not** on this list.
///
/// Skipping `Opci-uvjeti-2025-STARO.docx` looks tidy and quietly removes the
/// product's whole argument: the disagreement between last year's 8% and this
/// year's 5% is the thing the owner needs to see. Stage 3 already prefers the
/// newer document, so an old file costs one extra call and produces a
/// conflict the owner can resolve instead of a silence they cannot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    Personal,
    Template,
    TooShort,
}

impl Skip {
    /// What the owner is told. Each of these is something they can act on.
    pub fn reason(self) -> &'static str {
        match self {
            Self::Personal => "looks like personal or payroll data, and is held back",
            Self::Template => "is a blank template, so its sentences are not company decisions",
            Self::TooShort => "has too little text to state anything",
        }
    }
}

/// Whether this document should be sent to a model at all.
pub fn worth_reading(document: &Document) -> bool {
    classify(document).is_none()
}

/// Why this document is being skipped, if it is.
pub fn classify(document: &Document) -> Option<Skip> {
    let name = fold(&document.name);

    if PERSONAL_WORDS.iter().any(|word| name.contains(word)) {
        return Some(Skip::Personal);
    }
    if TEMPLATE_WORDS.iter().any(|word| name.contains(word)) {
        return Some(Skip::Template);
    }
    // A spreadsheet of prices is knowledge even when its prose is almost
    // nothing, so the length floor only applies to prose — and even there it
    // is one short sentence, not a paragraph. A company's payment terms are
    // sometimes a single line in a text file, and missing them costs far
    // more than the handful of pointless calls a low floor lets through.
    let floor = match document.kind {
        DocumentKind::Xlsx | DocumentKind::Csv => 1,
        _ => 30,
    };
    if document.text.trim().len() < floor {
        return Some(Skip::TooShort);
    }

    None
}

/// Lowercases and strips Croatian diacritics, so `STARO`, `Staro` and
/// `Šablona` all match.
fn fold(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'č' | 'ć' => 'c',
            'ž' => 'z',
            'š' => 's',
            'đ' => 'd',
            other => other,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(name: &str, text: &str) -> Document {
        knowlith_extract::extract_bytes(
            std::path::Path::new(name),
            text.as_bytes(),
            "2026-01-01T00:00:00Z",
        )
        .unwrap()
    }

    const PROSE: &str = "# Uvjeti\n\nPopust od 5% odobrava se stalnim kupcima koji su u prethodnih dvanaest mjeseci ostvarili promet veci od 20.000 EUR bez PDV-a.\n";

    #[test]
    fn a_normal_document_is_read() {
        assert!(worth_reading(&doc("/Opci-uvjeti-2026.md", PROSE)));
    }

    #[test]
    fn payroll_is_held_back() {
        assert_eq!(classify(&doc("/Zaposlenici-place-2026.md", PROSE)), Some(Skip::Personal));
    }

    #[test]
    fn a_blank_template_is_not_a_company_decision() {
        assert_eq!(classify(&doc("/Predlozak-ponude.md", PROSE)), Some(Skip::Template));
        assert_eq!(classify(&doc("/Predložak-ponude.md", PROSE)), Some(Skip::Template));
    }

    #[test]
    fn last_years_copy_is_read_so_the_disagreement_can_be_seen() {
        assert!(
            worth_reading(&doc("/Opci-uvjeti-2025-STARO.md", PROSE)),
            "skipping the old file hides the conflict instead of resolving it"
        );
    }

    #[test]
    fn a_price_list_is_read_even_with_almost_no_prose() {
        let sheet = doc("/Cjenik.csv", "Stavka,Cijena\nMontaza,210\n");
        assert!(worth_reading(&sheet), "a short spreadsheet is still a price list");
    }

    #[test]
    fn a_two_word_note_states_nothing() {
        assert_eq!(classify(&doc("/Biljeska.md", "Nazvati Anu.\n")), Some(Skip::TooShort));
    }

    #[test]
    fn a_single_sentence_rule_is_still_read() {
        let note = doc("/Rok.md", "Rok placanja je 15 dana od izdavanja racuna.\n");
        assert!(
            worth_reading(&note),
            "a company's payment terms are sometimes one line, and missing them is the expensive mistake"
        );
    }

    #[test]
    fn every_skip_reason_is_something_the_owner_can_act_on() {
        for skip in [Skip::Personal, Skip::Template, Skip::TooShort] {
            assert!(skip.reason().len() > 20, "a reason has to be a sentence");
        }
    }
}
