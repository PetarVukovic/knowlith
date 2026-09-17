//! Stage 3: deciding what is current.
//!
//! This is the stage that separates the product from retrieval. A search
//! engine can find both the 2023 policy and the 2026 one and hand back
//! whichever scored higher; it has no opinion about which one is true today.
//! Here the question is answered, deterministically, and the losing document
//! is kept and labelled rather than quietly outranked.
//!
//! No model is involved. Recency comes from the file's own modification
//! time, disagreement comes from comparing the figures two documents state,
//! and both are things the owner can check.

use std::collections::HashMap;

use knowlith_core::Document;

use crate::candidates::Candidate;

/// One subject, resolved.
#[derive(Debug, Clone)]
pub struct Group {
    /// Stable across runs: `rule:odobravanje-popusta`.
    pub id: String,
    pub candidate: Candidate,
    pub document_id: String,
    pub document_name: String,
    pub quotes: Vec<String>,
    /// How many distinct documents state the same thing. The only honest
    /// confidence signal available without a model.
    pub agreeing_documents: usize,
    pub conflicted: bool,
    /// Quotes from documents that state a different figure for the same subject.
    pub disagreeing: Vec<(String, String)>,
    /// The document whose version this replaces.
    pub supersedes: Option<String>,
    /// Other objects this one names.
    pub uses: Vec<String>,
}

impl Group {
    #[cfg(test)]
    pub fn for_test() -> Self {
        Self {
            id: "rule:test".into(),
            candidate: Candidate {
                kind: "rule".into(),
                title: "Test".into(),
                statement: "Test.".into(),
                subtype: None,
                quotes: vec!["Test.".into()],
            },
            document_id: "doc:test".into(),
            document_name: "test.md".into(),
            quotes: vec!["Test.".into()],
            agreeing_documents: 1,
            conflicted: false,
            disagreeing: Vec::new(),
            supersedes: None,
            uses: Vec::new(),
        }
    }
}

/// Two documents saying different things about one subject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub object_id: String,
    pub subject: String,
    pub sides: Vec<Side>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Side {
    pub document_id: String,
    pub document_name: String,
    pub modified: String,
    /// The figures the document states, when it states any. This is what the
    /// review screen puts side by side, because "8%" against "5%" is a
    /// decision an owner can make in a second.
    pub value: String,
    pub statement: String,
    pub quote: String,
    /// Whether this is the version being kept.
    pub current: bool,
}

/// Groups candidates by subject and resolves each group.
pub fn group(
    proposals: &[(String, Candidate)],
    documents: &HashMap<&str, &Document>,
) -> (Vec<Group>, Vec<Conflict>) {
    // Group by subject. The title is the first key, but it is a weak one —
    // a model reading two documents about one discount will title it
    // "Popust za stalne kupce" in one and "Redovni popust od 5%" in the
    // other, and grouping by title alone turned twelve documents into
    // sixty-nine objects, most of them the same rule wearing different
    // names.
    //
    // So a second, stronger key runs first: **two claims that cite the same
    // sentence are the same claim.** That is not a similarity heuristic, it
    // is an identity — there is one sentence and it says one thing.
    let mut by_quote: HashMap<String, String> = HashMap::new();
    let mut buckets: HashMap<String, Vec<(&str, &Candidate)>> = HashMap::new();

    for (document_id, candidate) in proposals {
        let mut key = object_id(candidate);
        for quote in &candidate.quotes {
            let anchor = format!("{}|{}", document_id, normalise_quote(quote));
            match by_quote.get(&anchor) {
                Some(existing) => {
                    key = existing.clone();
                    break;
                }
                None => {}
            }
        }
        for quote in &candidate.quotes {
            by_quote
                .entry(format!("{}|{}", document_id, normalise_quote(quote)))
                .or_insert_with(|| key.clone());
        }
        buckets.entry(key).or_default().push((document_id.as_str(), candidate));
    }

    let mut groups = Vec::new();
    let mut conflicts = Vec::new();

    for (id, mut members) in buckets {
        // Newest first. A document with no usable timestamp sorts last
        // rather than winning by accident.
        members.sort_by(|a, b| {
            let left = documents.get(a.0).map(|d| d.modified.as_str()).unwrap_or("");
            let right = documents.get(b.0).map(|d| d.modified.as_str()).unwrap_or("");
            right.cmp(left)
        });

        let (current_doc_id, current) = members[0];
        let Some(current_document) = documents.get(current_doc_id) else {
            continue;
        };

        let current_figures = figures(&current.statement);
        let mut agreeing = 1usize;
        let mut disagreeing: Vec<(&str, &Candidate)> = Vec::new();

        for (document_id, candidate) in &members[1..] {
            if disagrees(current_doc_id, current, document_id, candidate) {
                disagreeing.push((document_id, candidate));
            } else if *document_id != current_doc_id {
                agreeing += 1;
            }
        }

        let conflicted = !disagreeing.is_empty();
        if conflicted {
            let mut sides = vec![Side {
                document_id: current_doc_id.to_string(),
                document_name: current_document.name.clone(),
                modified: current_document.modified.clone(),
                value: current_figures.join(", "),
                statement: current.statement.clone(),
                quote: current.quotes.first().cloned().unwrap_or_default(),
                current: true,
            }];
            for (document_id, candidate) in &disagreeing {
                let Some(document) = documents.get(document_id) else {
                    continue;
                };
                sides.push(Side {
                    document_id: (*document_id).to_string(),
                    document_name: document.name.clone(),
                    modified: document.modified.clone(),
                    value: figures(&candidate.statement).join(", "),
                    statement: candidate.statement.clone(),
                    quote: candidate.quotes.first().cloned().unwrap_or_default(),
                    current: false,
                });
            }
            conflicts.push(Conflict {
                object_id: id.clone(),
                subject: current.title.clone(),
                sides,
            });
        }

        // Every quote from the documents that agree, so an object that three
        // documents state carries all three spans rather than one.
        let mut quotes = current.quotes.clone();
        for (document_id, candidate) in &members[1..] {
            if disagrees(current_doc_id, current, document_id, candidate) {
                continue;
            }
            for quote in &candidate.quotes {
                if !quotes.contains(quote) {
                    quotes.push(quote.clone());
                }
            }
        }

        groups.push(Group {
            id: id.clone(),
            candidate: current.clone(),
            document_id: current_doc_id.to_string(),
            document_name: current_document.name.clone(),
            quotes,
            agreeing_documents: agreeing,
            conflicted,
            disagreeing: disagreeing
                .iter()
                .filter_map(|(document_id, candidate)| {
                    candidate
                        .quotes
                        .first()
                        .map(|quote| (document_id.to_string(), quote.clone()))
                })
                .collect(),
            supersedes: disagreeing
                .first()
                .map(|(document_id, _)| (*document_id).to_string()),
            uses: Vec::new(),
        });
    }

    // Structural edges: one object naming another's subject. Cheap, reliable,
    // and enough to answer "what breaks if I change this" without asking a
    // model to guess at dependencies.
    let titles: Vec<(String, String)> = groups
        .iter()
        .map(|g| (g.id.clone(), fold(&g.candidate.title)))
        .collect();
    for candidate_group in groups.iter_mut() {
        let haystack = fold(&candidate_group.candidate.statement);
        let mut uses: Vec<String> = titles
            .iter()
            .filter(|(id, title)| {
                *id != candidate_group.id && title.len() >= 6 && haystack.contains(title.as_str())
            })
            .map(|(id, _)| id.clone())
            .collect();
        uses.sort();
        candidate_group.uses = uses;
    }

    groups.sort_by(|a, b| a.id.cmp(&b.id));
    conflicts.sort_by(|a, b| a.object_id.cmp(&b.object_id));
    (groups, conflicts)
}

/// `rule:odobravanje-popusta` — derived from the title so it survives a
/// recompile, and readable so a person can recognise it in a URL.
pub fn object_id(candidate: &Candidate) -> String {
    let kind = match candidate.object_kind() {
        knowlith_core::ObjectKind::Rule => "rule",
        knowlith_core::ObjectKind::Process => "process",
        knowlith_core::ObjectKind::Term => "term",
        knowlith_core::ObjectKind::Fact => "fact",
        // Unreachable: stage 2's schema has no "skill". Spelled out anyway,
        // because a silent fallback to "rule" here would be a wrong id.
        knowlith_core::ObjectKind::Skill => "skill",
    };
    format!("{kind}:{}", slug(&candidate.title))
}

/// Whether two candidates genuinely disagree.
///
/// Three conditions, and all three are required, because a conflict shown to
/// an owner has to be one they can resolve by choosing a document.
///
/// **Different documents.** A single document that states a discount in one
/// sentence and its turnover threshold in the next has not contradicted
/// itself — the model split one rule in two, and reporting that as a
/// disagreement teaches the owner to ignore the conflict screen.
///
/// **Both state figures.** Numeric disagreement is something this code can
/// actually establish: 8% is not 5%. Two differently worded paragraphs about
/// the handover record may or may not disagree, and no amount of string
/// comparison settles it. Calling that a conflict would be claiming a
/// finding we cannot support, which is the one thing this product must never
/// do — so without figures the newer document simply wins, quietly.
///
/// **The figures differ.** "5%" and "5 posto" are the same figure.
fn disagrees(a_document: &str, a: &Candidate, b_document: &str, b: &Candidate) -> bool {
    if a_document == b_document {
        return false;
    }
    let (left, right) = (figures(&a.statement), figures(&b.statement));
    if left.is_empty() || right.is_empty() {
        return false;
    }
    left != right
}

/// Every number in a statement, with its unit where one follows.
///
/// `%`, `dana`, `mjeseci`, `sati`, `EUR` are the units a small company's
/// rules are actually written in, and keeping the unit prevents "15 dana"
/// and "15%" from looking like the same figure.
pub(crate) fn figures(statement: &str) -> Vec<String> {
    let folded = fold(statement);
    let bytes: Vec<char> = folded.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        if !bytes[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == '.' || bytes[i] == ',') {
            i += 1;
        }
        let number: String = bytes[start..i].iter().collect();
        let number = canonical_number(number.trim_end_matches(['.', ',']));

        let rest: String = bytes[i..].iter().collect();
        let rest = rest.trim_start();
        let unit = ["%", "posto", "dana", "dan", "mjeseci", "mjesec", "sati", "sat", "eur", "kn"]
            .iter()
            .find(|unit| rest.starts_with(**unit))
            .map(|unit| match *unit {
                "posto" => "%",
                "dan" => "dana",
                "mjesec" => "mjeseci",
                "sat" => "sati",
                other => other,
            })
            .unwrap_or("");

        out.push(if unit.is_empty() {
            number
        } else {
            format!("{number}{unit}")
        });
    }

    out.sort();
    out.dedup();
    out
}

/// One amount, written one way.
///
/// Croatian documents write five thousand euro as `5.000`, `5.000,00` and
/// `5000`, and the same offer often uses two of them. Compared as strings
/// those are three different figures, which made the compiler report a
/// disagreement between a document and its own restatement — a conflict the
/// owner is asked to resolve by choosing between two identical amounts.
///
/// A dot is only treated as a thousands separator when exactly three digits
/// follow it, so `7.1` stays seven point one rather than becoming seventy
/// one. The comma is the decimal mark, as it is in the source material.
fn canonical_number(raw: &str) -> String {
    let mut digits = String::with_capacity(raw.len());
    let chars: Vec<char> = raw.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '.' => {
                let run = chars[i + 1..]
                    .iter()
                    .take_while(|c| c.is_ascii_digit())
                    .count();
                let ends_here = i + 1 + run >= chars.len();
                if run == 3 && ends_here {
                    // A thousands separator: drop it.
                } else if run == 3 && chars.get(i + 1 + run) == Some(&'.') {
                    // Another group follows — still a separator.
                } else if run == 3 && chars.get(i + 1 + run) == Some(&',') {
                    // `5.000,00`.
                } else {
                    digits.push('.');
                }
            }
            ',' => digits.push('.'),
            c => digits.push(c),
        }
        i += 1;
    }

    match digits.parse::<f64>() {
        Ok(value) => {
            let rendered = format!("{value:.2}");
            rendered
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_string()
        }
        Err(_) => raw.to_string(),
    }
}

fn slug(title: &str) -> String {
    let folded = fold(title);
    let mut out = String::new();
    let mut last_dash = true;
    for c in folded.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

/// A quote reduced to what it says, so the same sentence quoted with a
/// different amount of trailing punctuation still counts as the same
/// sentence.
fn normalise_quote(quote: &str) -> String {
    fold(quote)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_string()
}

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

    fn candidate(kind: &str, title: &str, statement: &str, quote: &str) -> Candidate {
        Candidate {
            kind: kind.into(),
            title: title.into(),
            statement: statement.into(),
            subtype: None,
            quotes: vec![quote.into()],
        }
    }

    fn document(id: &str, name: &str, modified: &str) -> Document {
        Document {
            id: id.into(),
            path: format!("/{name}"),
            name: name.into(),
            kind: knowlith_core::DocumentKind::Markdown,
            byte_len: 0,
            sha256: String::new(),
            text: String::new(),
            text_sha256: String::new(),
            verbatim: true,
            modified: modified.into(),
            columns: None,
            blocks: Vec::new(),
        }
    }

    #[test]
    fn the_same_subject_from_two_documents_is_one_object() {
        let a = document("doc:a", "novo.md", "2026-01-01T00:00:00Z");
        let b = document("doc:b", "staro.md", "2025-01-01T00:00:00Z");
        let docs = HashMap::from([("doc:a", &a), ("doc:b", &b)]);

        let proposals = vec![
            ("doc:a".to_string(), candidate("rule", "Odobravanje popusta", "Popust je 5%.", "A")),
            ("doc:b".to_string(), candidate("rule", "Odobravanje popusta", "Popust je 5%.", "B")),
        ];

        let (groups, conflicts) = group(&proposals, &docs);
        assert_eq!(groups.len(), 1);
        assert!(conflicts.is_empty(), "agreement is not a conflict");
        assert_eq!(groups[0].agreeing_documents, 2);
        assert_eq!(groups[0].quotes.len(), 2, "both spans are kept");
    }

    #[test]
    fn different_figures_about_one_subject_are_a_conflict_the_newer_one_wins() {
        let a = document("doc:a", "novo.md", "2026-01-01T00:00:00Z");
        let b = document("doc:b", "staro.md", "2023-01-01T00:00:00Z");
        let docs = HashMap::from([("doc:a", &a), ("doc:b", &b)]);

        let proposals = vec![
            ("doc:b".to_string(), candidate("rule", "Odobravanje popusta", "Popust je 8%.", "osam")),
            ("doc:a".to_string(), candidate("rule", "Odobravanje popusta", "Popust je 5%.", "pet")),
        ];

        let (groups, conflicts) = group(&proposals, &docs);
        assert_eq!(groups.len(), 1);
        assert!(groups[0].conflicted);
        assert!(groups[0].candidate.statement.contains("5%"));
        assert_eq!(groups[0].supersedes.as_deref(), Some("doc:b"));

        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].sides.len(), 2);
        assert_eq!(conflicts[0].sides[0].value, "5%");
        assert!(conflicts[0].sides[0].current);
        assert_eq!(conflicts[0].sides[1].value, "8%");
        assert!(!conflicts[0].sides[1].current);
    }

    #[test]
    fn a_document_does_not_conflict_with_itself() {
        let a = document("doc:a", "uvjeti.md", "2026-01-01T00:00:00Z");
        let docs = HashMap::from([("doc:a", &a)]);
        let sentence = "Popust od 5% za kupce s prometom iznad 20.000 EUR.";

        let proposals = vec![
            ("doc:a".to_string(), candidate("rule", "Popust", "Popust je 5%.", sentence)),
            (
                "doc:a".to_string(),
                candidate("rule", "Prag prometa", "Prag je 20.000 EUR.", sentence),
            ),
        ];

        let (_, conflicts) = group(&proposals, &docs);
        assert!(
            conflicts.is_empty(),
            "one document splitting a rule in two is not a disagreement"
        );
    }

    #[test]
    fn prose_that_merely_reads_differently_is_not_claimed_as_a_conflict() {
        let a = document("doc:a", "novo.md", "2026-01-01T00:00:00Z");
        let b = document("doc:b", "staro.md", "2025-01-01T00:00:00Z");
        let docs = HashMap::from([("doc:a", &a), ("doc:b", &b)]);

        let proposals = vec![
            (
                "doc:a".to_string(),
                candidate("term", "Primopredajni zapisnik", "Dokument o preuzimanju radova.", "x"),
            ),
            (
                "doc:b".to_string(),
                candidate("term", "Primopredajni zapisnik", "Zapisnik koji potpisuju obje strane.", "y"),
            ),
        ];

        let (groups, conflicts) = group(&proposals, &docs);
        assert!(
            conflicts.is_empty(),
            "without figures we cannot establish a disagreement, so we do not assert one"
        );
        assert!(groups[0].candidate.statement.contains("preuzimanju"), "the newer one is kept");
    }

    #[test]
    fn the_same_figure_worded_differently_is_not_a_conflict() {
        let a = document("doc:a", "a.md", "2026-01-01T00:00:00Z");
        let b = document("doc:b", "b.md", "2025-01-01T00:00:00Z");
        let docs = HashMap::from([("doc:a", &a), ("doc:b", &b)]);

        let proposals = vec![
            ("doc:a".to_string(), candidate("rule", "Popust", "Popust je 5%.", "x")),
            ("doc:b".to_string(), candidate("rule", "Popust", "Odobrava se popust od 5 posto.", "y")),
        ];

        let (_, conflicts) = group(&proposals, &docs);
        assert!(conflicts.is_empty(), "5% and 5 posto are the same figure");
    }

    #[test]
    fn days_and_percentages_are_not_the_same_figure() {
        assert_ne!(figures("Rok je 15 dana."), figures("Popust je 15%."));
        assert_eq!(figures("Rok je 15 dana."), vec!["15dana"]);
    }

    #[test]
    fn an_object_that_names_another_depends_on_it() {
        let a = document("doc:a", "a.md", "2026-01-01T00:00:00Z");
        let docs = HashMap::from([("doc:a", &a)]);
        let proposals = vec![
            ("doc:a".to_string(), candidate("rule", "Odobravanje popusta", "Popust je 5%.", "q1")),
            (
                "doc:a".to_string(),
                candidate(
                    "process",
                    "Izrada ponude",
                    "Prodavac primjenjuje odobravanje popusta prije slanja.",
                    "q2",
                ),
            ),
        ];

        let (groups, _) = group(&proposals, &docs);
        let process = groups.iter().find(|g| g.id.starts_with("process:")).unwrap();
        assert_eq!(process.uses, ["rule:odobravanje-popusta"]);
    }

    #[test]
    fn two_titles_for_one_sentence_are_one_object() {
        let a = document("doc:a", "uvjeti.md", "2026-01-01T00:00:00Z");
        let docs = HashMap::from([("doc:a", &a)]);
        let sentence = "Popust od 5% odobrava se stalnim kupcima.";

        let proposals = vec![
            (
                "doc:a".to_string(),
                candidate("rule", "Popust za stalne kupce", "Popust je 5%.", sentence),
            ),
            (
                "doc:a".to_string(),
                candidate("rule", "Redovni popust od 5%", "Redovni popust iznosi 5%.", sentence),
            ),
        ];

        let (groups, _) = group(&proposals, &docs);
        assert_eq!(
            groups.len(),
            1,
            "one sentence states one thing, however the model titles it"
        );
    }

    #[test]
    fn two_different_sentences_stay_two_objects() {
        let a = document("doc:a", "uvjeti.md", "2026-01-01T00:00:00Z");
        let docs = HashMap::from([("doc:a", &a)]);

        let proposals = vec![
            ("doc:a".to_string(), candidate("rule", "Popust", "Popust je 5%.", "Popust od 5%.")),
            ("doc:a".to_string(), candidate("rule", "Rok", "Rok je 15 dana.", "Rok placanja je 15 dana.")),
        ];

        assert_eq!(group(&proposals, &docs).0.len(), 2);
    }

    #[test]
    fn ids_are_readable_and_survive_diacritics() {
        assert_eq!(
            object_id(&candidate("rule", "Rok plaćanja i avans", "x", "y")),
            "rule:rok-placanja-i-avans"
        );
    }

    #[test]
    fn a_document_with_no_timestamp_does_not_win_by_accident() {
        let dated = document("doc:a", "dated.md", "2026-01-01T00:00:00Z");
        let undated = document("doc:b", "undated.md", "");
        let docs = HashMap::from([("doc:a", &dated), ("doc:b", &undated)]);

        let proposals = vec![
            ("doc:b".to_string(), candidate("rule", "Popust", "Popust je 9%.", "x")),
            ("doc:a".to_string(), candidate("rule", "Popust", "Popust je 5%.", "y")),
        ];

        let (groups, _) = group(&proposals, &docs);
        assert!(groups[0].candidate.statement.contains("5%"));
    }
}

#[cfg(test)]
mod number_tests {
    use super::figures;

    #[test]
    fn one_amount_written_three_ways_is_one_figure() {
        assert_eq!(figures("Avans za radove iznad 5.000 EUR."), vec!["5000eur"]);
        assert_eq!(figures("Avans za radove iznad 5.000,00 EUR."), vec!["5000eur"]);
        assert_eq!(figures("Avans za radove iznad 5000 EUR."), vec!["5000eur"]);
    }

    #[test]
    fn a_decimal_written_with_a_dot_is_not_read_as_thousands() {
        // A model transcribing "7,1 kW" as "7.1 kW" must not turn it into 71.
        assert_eq!(figures("Kanalska jedinica 7.1"), vec!["7.1"]);
        assert_eq!(figures("Kanalska jedinica 7,1"), vec!["7.1"]);
    }

    #[test]
    fn genuinely_different_amounts_stay_different() {
        assert_ne!(figures("Rok plaćanja je 15 dana."), figures("Rok plaćanja je 30 dana."));
    }

    #[test]
    fn a_unit_still_separates_two_fifteens() {
        assert_ne!(figures("popust 15%"), figures("rok 15 dana"));
    }
}
