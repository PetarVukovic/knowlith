//! Pairs of objects that may be one thing said twice.
//!
//! Stage 3 merges claims that cite the same sentence. That is an identity,
//! not a similarity, and it is why the compiler can merge at all without
//! risking a wrong merge. What it leaves behind is the case where two
//! documents word one rule differently and share no sentence: "Popust za
//! stalne kupce" in one, "Redovni popust od 5%" in the other, and the folder
//! ends up with both.
//!
//! The obvious fix — lower the bar until those two group together — is the
//! wrong one. It makes a similarity score the thing that decides two of the
//! company's rules are one rule, and a merge that should not have happened
//! deletes a rule so quietly that nobody finds out until an agent answers
//! with the wrong one.
//!
//! So this module is deliberately built the other way round. It is tuned for
//! **recall, not precision**, because it never acts: everything it produces
//! is a question the owner answers once. A false pair costs one dismissal. A
//! missed pair costs a duplicate that stays forever.

use std::collections::BTreeSet;

use knowlith_core::{ContextObject, ObjectStatus};

use crate::consolidate::figures;

/// What kind of question a pair raises.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HintKind {
    /// One decision written twice. Merging loses nothing.
    Duplicate,
    /// One subject, two different figures.
    ///
    /// Stage 3 finds this only inside a group, and a group is keyed by
    /// subject identity — so two documents stating 5% and 8% for the same
    /// discount, under two different titles, went through the whole
    /// compiler without anyone noticing. That is the worst possible outcome
    /// for this product: not a missed duplicate, a missed contradiction,
    /// with both numbers live and an agent free to quote either.
    Disagreement,
}

/// A pair worth asking about.
#[derive(Debug, Clone, PartialEq)]
pub struct Hint {
    /// The one to keep if the owner says yes: better evidenced, and on a tie,
    /// the newer.
    pub keep_id: String,
    pub drop_id: String,
    pub kind: HintKind,
    pub score: f32,
}

/// Words too common in Croatian business prose to carry any subject.
///
/// Without this, "Rok plaćanja po ponudi" and "Rok isporuke po ugovoru" score
/// on `rok` and `po` and look like the same rule.
const STOP: &[&str] = &[
    "i", "ili", "je", "su", "se", "na", "u", "za", "od", "do", "sa", "s", "po", "kod", "pri",
    "koji", "koja", "koje", "kojih", "sto", "kada", "ako", "te", "a", "ali", "da", "ne", "nije",
    "moze", "mora", "treba", "biti", "bude", "svaki", "svaka", "sve", "ovaj", "ova", "ovo", "the",
];

/// Below this, two objects are not the same subject in any reading.
const THRESHOLD: f32 = 0.5;

/// Croatian inflects almost every noun, and a rule's subject survives that
/// inflection while an exact word match does not: `krugovi` and `krugova`
/// are one concept and two strings. Comparing five-character stems is crude
/// morphology and it is the difference between finding the duplicates in a
/// real folder and finding one of them.
const STEM: usize = 5;

/// Finds every pair worth asking the owner about.
///
/// Two conditions, each of which exists because dropping it produced a
/// question the owner could not answer:
///
/// * **Same kind.** A term and a rule about the same subject are not
///   duplicates; one defines a word and the other sets a limit.
/// * **Neither already superseded or rejected.** Those decisions were made.
///
/// Whether the two state different figures decides which question is asked,
/// not whether one is asked at all.
pub fn hints(objects: &[ContextObject]) -> Vec<Hint> {
    let live: Vec<&ContextObject> = objects
        .iter()
        .filter(|o| {
            matches!(
                o.status,
                ObjectStatus::Proposed | ObjectStatus::Conflicted | ObjectStatus::Approved
            )
        })
        .collect();

    let profiles: Vec<Profile> = live
        .iter()
        .enumerate()
        .map(|(index, object)| Profile {
            index,
            title: content_words(&object.title),
            body: content_words(&object.body),
            figures: figures(&object.body),
        })
        .collect();

    let mut out = Vec::new();

    for left_profile in &profiles {
        for right_profile in &profiles {
            let (i, j) = (&left_profile.index, &right_profile.index);
            let (left_figures, right_figures) = (&left_profile.figures, &right_profile.figures);
            if j <= i {
                continue;
            }
            let (left, right) = (live[*i], live[*j]);
            if left.kind != right.kind {
                continue;
            }
            let differing_figures = !left_figures.is_empty()
                && !right_figures.is_empty()
                && left_figures != right_figures;

            // A disagreement is held to a stricter bar than a duplicate,
            // because the two mistakes cost differently. A wrongly offered
            // duplicate costs one dismissal. A wrongly asserted
            // disagreement tells the owner their documents contradict each
            // other when they do not, which is the one claim this product
            // must never make.
            //
            // The bar is that neither title carries a content word the
            // other lacks. "Cijena montaže split sustava" and "Cijena
            // montaže multi split sustava" name two products at two prices,
            // and `multi` is exactly what distinguishes them; there is no
            // way to rule that out, so it is not reported. "Popust za
            // stalne kupce" and "Popust od 5% za stalne kupce" reduce to
            // the same words, and 5% against 8% for those customers is a
            // contradiction nobody chose.
            if differing_figures && left_profile.title != right_profile.title {
                continue;
            }

            let score = subject_overlap(left_profile, right_profile);
            if score < THRESHOLD {
                continue;
            }

            let (keep, drop) = preferred(left, right);
            out.push(Hint {
                keep_id: keep.id.clone(),
                drop_id: drop.id.clone(),
                kind: if differing_figures {
                    HintKind::Disagreement
                } else {
                    HintKind::Duplicate
                },
                score,
            });
        }
    }

    // A contradiction outranks a duplicate at any score. One is two entries
    // where there should be one; the other is two live answers to the same
    // question, and an agent asked today would pick whichever it saw first.
    out.sort_by(|a, b| {
        (a.kind == HintKind::Duplicate)
            .cmp(&(b.kind == HintKind::Duplicate))
            .then_with(|| b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| a.keep_id.cmp(&b.keep_id))
    });
    out
}

/// Which side survives a merge.
///
/// More source quotes first: the better-evidenced wording is the one worth
/// keeping. On a tie, the more recent, and on a tie there, the lower id so
/// two runs over the same folder do not disagree with each other.
fn preferred<'a>(left: &'a ContextObject, right: &'a ContextObject) -> (&'a ContextObject, &'a ContextObject) {
    let left_first = (left.evidence.len(), left.valid_from.as_str(), std::cmp::Reverse(left.id.as_str()))
        >= (right.evidence.len(), right.valid_from.as_str(), std::cmp::Reverse(right.id.as_str()));
    if left_first { (left, right) } else { (right, left) }
}

struct Profile {
    index: usize,
    title: BTreeSet<String>,
    body: BTreeSet<String>,
    figures: Vec<String>,
}

/// How much two objects are about the same thing.
///
/// The title carries most of the weight, and it is scored by **containment**
/// rather than by Jaccard. "Popust za stalne kupce" and "Popust od 5% za
/// stalne kupce" are the same rule with a qualifier added, and Jaccard
/// punishes exactly that: the longer title's extra words count against a
/// match they have nothing to do with. Containment asks the question that
/// matters — is one subject wholly inside the other.
///
/// A one-word title would score a perfect containment against any title
/// containing that word, which is a match on nothing, so containment needs
/// at least two stems on the shorter side and falls back to Jaccard below
/// that. The body then has to agree as well, which is what stops two
/// different rules with similar names from being offered.
fn subject_overlap(left: &Profile, right: &Profile) -> f32 {
    let smaller = left.title.len().min(right.title.len());
    let shared = left.title.intersection(&right.title).count() as f32;
    let title = if smaller >= 2 {
        shared / smaller as f32
    } else {
        jaccard(&left.title, &right.title)
    };
    0.65 * title + 0.35 * jaccard(&left.body, &right.body)
}

fn jaccard(left: &BTreeSet<String>, right: &BTreeSet<String>) -> f32 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let shared = left.intersection(right).count() as f32;
    let total = left.union(right).count() as f32;
    shared / total
}

/// Diacritic-folded, five-character stems of content words.
///
/// Four characters minimum before stemming: "PDV" and "rok" are everywhere
/// and carry no subject. Folding diacritics means "placanje" and "plaćanje"
/// are the same word, which they are; truncating to five means "placanja"
/// is too, which it also is.
fn content_words(text: &str) -> BTreeSet<String> {
    text.chars()
        .map(fold_char)
        .collect::<String>()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.chars().count() >= 4 && !STOP.contains(word))
        .map(|word| word.chars().take(STEM).collect::<String>())
        .collect()
}

fn fold_char(c: char) -> char {
    match c.to_ascii_lowercase() {
        'č' | 'ć' => 'c',
        'ž' => 'z',
        'š' => 's',
        'đ' => 'd',
        other => {
            let lowered: Vec<char> = other.to_lowercase().collect();
            match lowered.as_slice() {
                ['č'] | ['ć'] => 'c',
                ['ž'] => 'z',
                ['š'] => 's',
                ['đ'] => 'd',
                [one] => *one,
                _ => other,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use knowlith_core::{Confidence, Evidence, ObjectKind};

    fn object(id: &str, title: &str, body: &str, kind: ObjectKind, spans: usize) -> ContextObject {
        ContextObject {
            id: id.into(),
            kind,
            subtype: None,
            title: title.into(),
            body: body.into(),
            status: ObjectStatus::Proposed,
            confidence: Confidence(0.7),
            version: 1,
            valid_from: "2026-01-01T00:00:00Z".into(),
            valid_to: None,
            supersedes: None,
            decided_by: None,
            edited_on_approval: false,
            evidence: (0..spans)
                .map(|i| Evidence {
                    document_id: format!("doc:{i}"),
                    locator: "§1 ¶1".into(),
                    start_byte: i,
                    end_byte: i + 1,
                    quote: "x".into(),
                })
                .collect(),
            relations: Vec::new(),
            path: "rules/x.md".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn one_rule_written_twice_is_offered_as_a_pair() {
        let objects = vec![
            object(
                "rule:popust-stalni-kupci",
                "Popust za stalne kupce",
                "Stalnim kupcima odobrava se popust od 5% na redovnu cijenu.",
                ObjectKind::Rule,
                1,
            ),
            object(
                "rule:redovni-popust",
                "Redovni popust stalnim kupcima",
                "Stalnim kupcima odobrava se popust 5% na redovnu cijenu opreme.",
                ObjectKind::Rule,
                2,
            ),
        ];
        let found = hints(&objects);
        assert_eq!(found.len(), 1);
        // The better-evidenced wording survives.
        assert_eq!(found[0].keep_id, "rule:redovni-popust");
    }

    #[test]
    fn two_rules_about_different_subjects_are_not_offered() {
        let objects = vec![
            object("rule:rok-placanja", "Rok plaćanja", "Rok plaćanja je 15 dana.", ObjectKind::Rule, 1),
            object("rule:rok-isporuke", "Rok isporuke", "Rok isporuke je 15 dana.", ObjectKind::Rule, 1),
        ];
        assert!(hints(&objects).is_empty());
    }

    /// The failure this module was extended to catch.
    #[test]
    fn two_titles_for_one_subject_with_two_figures_is_reported_as_a_disagreement() {
        let objects = vec![
            object(
                "rule:popust-za-stalne-kupce",
                "Popust za stalne kupce",
                "Stalnim kupcima odobrava se popust od 8%.",
                ObjectKind::Rule,
                1,
            ),
            object(
                "rule:popust-od-5-za-stalne-kupce",
                "Popust od 5% za stalne kupce",
                "Stalnim kupcima odobrava se popust od 5%.",
                ObjectKind::Rule,
                1,
            ),
        ];
        let found = hints(&objects);
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].kind,
            HintKind::Disagreement,
            "5% and 8% for the same customers is not a duplicate"
        );
    }

    /// The precision guard. Two products, two prices, one extra word.
    #[test]
    fn a_qualifier_that_may_be_the_difference_is_never_called_a_contradiction() {
        let objects = vec![
            object(
                "fact:cijena-split",
                "Cijena montaže split sustava",
                "Montaža split sustava stoji 180 EUR.",
                ObjectKind::Fact,
                1,
            ),
            object(
                "fact:cijena-multi-split",
                "Cijena montaže multi split sustava",
                "Montaža multi split sustava stoji 320 EUR.",
                ObjectKind::Fact,
                1,
            ),
        ];
        assert!(
            hints(&objects).is_empty(),
            "`multi` may be exactly what separates these two prices"
        );
    }

    #[test]
    fn a_contradiction_is_offered_before_a_duplicate() {
        let objects = vec![
            object("rule:a", "Popust za stalne kupce", "Popust od 5% za stalne kupce.", ObjectKind::Rule, 1),
            object("rule:b", "Popust za stalne kupce redovni", "Popust od 5% za stalne kupce.", ObjectKind::Rule, 1),
            object("rule:c", "Jamstveni rok ugradnje", "Jamstveni rok na ugradnju je 24 mjeseca.", ObjectKind::Rule, 1),
            object("rule:d", "Jamstveni rok ugradnja", "Jamstveni rok na ugradnju je 12 mjeseci.", ObjectKind::Rule, 1),
        ];
        let found = hints(&objects);
        assert!(found.len() >= 2);
        assert_eq!(found[0].kind, HintKind::Disagreement);
    }

    #[test]
    fn a_term_and_a_rule_are_never_the_same_thing() {
        let objects = vec![
            object("term:popust", "Popust za stalne kupce", "Popust za stalne kupce označava redovni popust.", ObjectKind::Term, 1),
            object("rule:popust", "Popust za stalne kupce", "Popust za stalne kupce označava redovni popust.", ObjectKind::Rule, 1),
        ];
        assert!(hints(&objects).is_empty());
    }

    #[test]
    fn diacritics_and_inflection_do_not_hide_a_duplicate() {
        // One concept, three spellings, one stem.
        for spelling in ["plaćanja", "placanje", "PLAĆANJU"] {
            assert!(content_words(spelling).contains("placa"), "{spelling}");
        }
    }

    #[test]
    fn a_qualifier_added_to_a_title_does_not_hide_the_same_rule() {
        let objects = vec![
            object(
                "rule:krugovi-izmjena",
                "Krugovi izmjena ponude",
                "Ponuda uključuje dva kruga izmjena.",
                ObjectKind::Rule,
                1,
            ),
            object(
                "rule:ogranicenje-krugova",
                "Ograničenje broja krugova izmjena ponude",
                "Ponuda uključuje dva kruga izmjena bez naplate.",
                ObjectKind::Rule,
                1,
            ),
        ];
        assert_eq!(hints(&objects).len(), 1, "a longer title is the same subject with a qualifier");
    }

    #[test]
    fn a_rejected_object_is_not_offered_again() {
        let mut objects = vec![
            object("rule:a", "Popust za stalne kupce", "Popust od 5% za stalne kupce.", ObjectKind::Rule, 1),
            object("rule:b", "Popust za stalne kupce", "Popust od 5% za stalne kupce.", ObjectKind::Rule, 1),
        ];
        objects[1].status = ObjectStatus::Rejected;
        assert!(hints(&objects).is_empty());
    }
}
