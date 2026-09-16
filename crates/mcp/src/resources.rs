//! What a client can attach without asking the model to fetch it.
//!
//! Resources are chosen by the application, not by the model, which makes
//! them the right home for exactly one thing: the short card that always
//! applies. A client that supports resources can keep the company's terms
//! and standing rules in front of the model all the time, and then the
//! tools only have to answer what is specific.
//!
//! Documents are here as well, but as links rather than as an inventory:
//! the tools return `resource_link`s pointing at them, so the agent can
//! open the source of an answer without every reply carrying a contract.

use knowlith_core::{ObjectKind, ObjectStatus};
use knowlith_lake::Lake;
use serde_json::{Value, json};

use crate::gate;

pub const COMPANY_URI: &str = "knowlith://company";
const DOCUMENT_PREFIX: &str = "knowlith://document/";

/// How much of the company card is worth always having loaded.
///
/// Fifty-one objects fit in a context window and three thousand do not, and
/// the moment a card stops fitting it stops being read. So the card is the
/// vocabulary plus the rules nothing else rests on — the things that are
/// true regardless of what was asked — and everything else is a search.
const CARD_LIMIT: usize = 24;

pub fn list(lake: &Lake, company: &str, icon: &Value) -> Vec<Value> {
    let mut out = vec![json!({
        "uri": COMPANY_URI,
        "name": "company-card",
        "title": format!("{company} — what always applies"),
        "description": "This company's vocabulary and its standing rules, short enough to keep loaded.",
        "mimeType": "text/markdown",
        "icons": [icon],
        "annotations": { "audience": ["assistant"], "priority": 0.9 },
    })];

    for document in lake.documents().unwrap_or_default().into_iter().take(200) {
        out.push(json!({
            "uri": format!("{DOCUMENT_PREFIX}{}", document.id),
            "name": document.name,
            "title": document.name,
            "description": format!("The text Knowlith read out of {}.", document.name),
            "mimeType": "text/plain",
            "annotations": { "audience": ["assistant"], "priority": 0.2 },
        }));
    }
    out
}

/// `resources/templates/list`, so a client knows document URIs are legal
/// without us listing every one.
pub fn templates() -> Vec<Value> {
    vec![json!({
        "uriTemplate": "knowlith://document/{documentId}",
        "name": "source-document",
        "title": "A document this company gave Knowlith",
        "description": "The deterministic text every quote and offset refers to.",
        "mimeType": "text/plain",
    })]
}

/// `resources/read`. `None` when the URI is not one of ours.
pub fn read(lake: &Lake, company: &str, uri: &str) -> Option<Vec<Value>> {
    if uri == COMPANY_URI {
        return Some(vec![json!({
            "uri": uri,
            "mimeType": "text/markdown",
            "text": card(lake, company),
        })]);
    }

    let id = uri.strip_prefix(DOCUMENT_PREFIX)?;
    // The id is used as a key, never as a path. A URI cannot reach a file
    // outside the lake here because nothing here ever opens a file.
    let document = lake.document(id).ok()?;
    Some(vec![json!({
        "uri": uri,
        "mimeType": "text/plain",
        "text": document.text,
    })])
}

/// The always-applies card.
fn card(lake: &Lake, company: &str) -> String {
    let objects = lake.objects().unwrap_or_default();
    let servable: Vec<_> = objects.iter().filter(|o| gate::is_servable(o)).collect();

    if servable.is_empty() {
        return format!(
            "# {company}\n\nNothing has been approved in Knowlith yet. Do not answer questions about how this company works from general knowledge — say there is nothing recorded.\n"
        );
    }

    let edges = lake.edges().unwrap_or_default();
    let rests_on_something = |id: &str| edges.iter().any(|e| e.from_id == id);

    let mut card = format!(
        "# {company}\n\nWhat follows is approved by the owner. Where it gives a figure, that figure is the company's and is not to be adjusted. For anything not here, use the Knowlith tools; do not fill the gap from general knowledge.\n"
    );

    let terms: Vec<_> = servable
        .iter()
        .filter(|o| o.kind == ObjectKind::Term)
        .take(CARD_LIMIT)
        .collect();
    if !terms.is_empty() {
        card.push_str("\n## What words mean here\n\n");
        for term in terms {
            card.push_str(&format!("- **{}** — {}\n", term.title, one_line(&term.body)));
        }
    }

    // A rule that rests on nothing else is a rule that always applies. One
    // that rests on a threshold does not, and putting it on a card without
    // its threshold is how an agent quotes a discount nobody qualifies for.
    let standing: Vec<_> = servable
        .iter()
        .filter(|o| o.kind == ObjectKind::Rule && !rests_on_something(&o.id))
        .take(CARD_LIMIT)
        .collect();
    if !standing.is_empty() {
        card.push_str("\n## Rules that always apply\n\n");
        for rule in standing {
            card.push_str(&format!("- **{}** — {}\n", rule.title, one_line(&rule.body)));
        }
    }

    let open: Vec<_> = objects
        .iter()
        .filter(|o| matches!(o.status, ObjectStatus::Conflicted | ObjectStatus::Proposed))
        .take(CARD_LIMIT)
        .collect();
    if !open.is_empty() {
        card.push_str("\n## Open — the owner has not decided these\n\nName them as open questions. Do not answer them.\n\n");
        for object in open {
            card.push_str(&format!("- {}\n", object.title));
        }
    }

    card
}

fn one_line(body: &str) -> String {
    let text: String = body
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect::<Vec<_>>()
        .join(" ");
    text.chars().take(220).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_company_says_so_rather_than_offering_a_blank_card() {
        let lake = Lake::in_memory().unwrap();
        let card = card(&lake, "Termoval d.o.o.");
        assert!(card.contains("Nothing has been approved"));
        assert!(
            card.to_lowercase().contains("do not"),
            "the card has to forbid filling the gap"
        );
    }

    #[test]
    fn a_document_uri_that_is_not_ours_is_not_read() {
        let lake = Lake::in_memory().unwrap();
        assert!(read(&lake, "Termoval", "file:///etc/passwd").is_none());
        assert!(read(&lake, "Termoval", "knowlith://something-else").is_none());
    }

    #[test]
    fn the_company_card_is_always_listed_first() {
        let lake = Lake::in_memory().unwrap();
        let icon = json!({ "src": "data:," });
        let listed = list(&lake, "Termoval", &icon);
        assert_eq!(listed[0]["uri"], COMPANY_URI);
    }

    #[test]
    fn a_body_becomes_one_line_of_bounded_length() {
        let body = "# Naslov\n\nprva linija\n\ndruga linija";
        assert_eq!(one_line(body), "prva linija druga linija");
        assert!(one_line(&"a".repeat(500)).chars().count() <= 220);
    }
}
