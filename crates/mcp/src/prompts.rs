//! Skills, as something a person reaches for.
//!
//! A tool is chosen by the model; a prompt is chosen by the person, and
//! appears in their chat box after a slash. The same approved procedure is
//! offered both ways on purpose, because the two are genuinely different
//! situations: the owner typing `/knowlith` because they know they are
//! about to write a quote, and an agent halfway through a task realising it
//! needs the company's procedure.
//!
//! The built-in prompts exist for the first minute after connecting. An
//! owner who has just clicked "Connect" needs one thing to type that proves
//! the whole chain works, and "ask it something" is not that.

use knowlith_core::{ContextObject, ObjectKind};
use knowlith_lake::Lake;
use serde_json::{Value, json};

use crate::gate;

/// The prompts that exist whatever is in the lake.
///
/// Named in English like everything else in this repository; what the owner
/// reads is the `title`, which is theirs.
const BUILT_IN: [(&str, &str, &str); 3] = [
    (
        "check-against-company",
        "Check this against our rules",
        "Takes something you are about to send and checks it against everything this company has decided, naming what it could not check.",
    ),
    (
        "what-do-we-charge",
        "What do we charge for this?",
        "Reads the figure out of the company's own price list, with the row it came from.",
    ),
    (
        "how-do-we-do-this",
        "How do we do this here?",
        "The company's own procedure for something, in the order they actually do it.",
    ),
];

/// `prompts/list`.
pub fn list(lake: &Lake, icon: &Value) -> Vec<Value> {
    let mut out: Vec<Value> = BUILT_IN
        .iter()
        .map(|(name, title, description)| {
            json!({
                "name": name,
                "title": title,
                "description": description,
                "arguments": [{
                    "name": "situation",
                    "description": "What you are working on.",
                    "required": false,
                }],
                "icons": [icon],
            })
        })
        .collect();

    for skill in skills(lake) {
        out.push(json!({
            "name": slug(&skill.title),
            "title": skill.title,
            "description": first_line(&skill.body),
            "arguments": [{
                "name": "situation",
                "description": "Anything specific about this case.",
                "required": false,
            }],
            "icons": [icon],
        }));
    }
    out
}

/// `prompts/get`. Returns the messages, or `None` when the name is unknown.
pub fn get(lake: &mut Lake, name: &str, arguments: &Value) -> Option<(String, Vec<Value>)> {
    let situation = arguments
        .get("situation")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty());

    if let Some((_, title, _)) = BUILT_IN.iter().find(|(n, ..)| *n == name) {
        return Some((title.to_string(), vec![user_message(&built_in_text(name, situation))]));
    }

    let skill = skills(lake).into_iter().find(|s| slug(&s.title) == name)?;
    let _ = lake.record_case_read(&skill.id, "prompt", None);

    let mut text = format!(
        "Follow this company's own procedure, which its owner approved. It is not a suggestion and it is not to be improved on — where it states a figure, that figure is the company's.\n\n{}\n",
        skill.body.trim()
    );
    if let Some(situation) = situation {
        text.push_str(&format!("\nThis case: {situation}\n"));
    }
    text.push_str(
        "\nBefore you finish: call get_relevant_context for this case, read what it lists, and call check_coverage. If anything it names is an open question, say so rather than deciding it yourself.",
    );

    Some((skill.title.clone(), vec![user_message(&text)]))
}

fn built_in_text(name: &str, situation: Option<&str>) -> String {
    let about = situation.unwrap_or("what I am about to send");
    match name {
        "check-against-company" => format!(
            "Check {about} against this company's approved knowledge.\n\n\
             Work in this order:\n\
             1. get_relevant_context with what this is about, to see everything that touches it.\n\
             2. Read each one with get_context. Take every figure from lookup_value, never from a sentence.\n\
             3. check_coverage before you answer.\n\n\
             Report three things separately: what agrees with the company's rules, what contradicts them and which document says so, and what you could not check because the owner has not decided it."
        ),
        "what-do-we-charge" => format!(
            "What does this company charge for {about}?\n\n\
             Use lookup_value and read the figure out of the row. Give the amount, the document and the row it is in. \
             If there is no row for it, say that it is not in the price list — do not estimate, and do not interpolate between two prices you can see."
        ),
        "how-do-we-do-this" => format!(
            "How does this company handle {about}?\n\n\
             Start with get_relevant_context, then get_process. Give the steps in the company's own order and quote the document each step comes from. \
             Where the procedure depends on a rule, read the rule too and say what it says."
        ),
        _ => String::new(),
    }
}

fn user_message(text: &str) -> Value {
    json!({
        "role": "user",
        "content": { "type": "text", "text": text },
    })
}

fn skills(lake: &Lake) -> Vec<ContextObject> {
    lake.objects()
        .unwrap_or_default()
        .into_iter()
        .filter(|o| o.kind == ObjectKind::Skill && gate::is_servable(o))
        .collect()
}

fn first_line(body: &str) -> String {
    body.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .unwrap_or("A procedure this company approved.")
        .chars()
        .take(160)
        .collect()
}

/// A title as a slash command.
///
/// Diacritics folded because a slash command is typed, and a command an
/// owner has to produce a `ć` for is one they will type wrong twice and
/// then stop using.
pub fn slug(title: &str) -> String {
    let folded: String = title
        .chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| match c {
            'č' | 'ć' => 'c',
            'ž' => 'z',
            'š' => 's',
            'đ' => 'd',
            c if c.is_alphanumeric() => c,
            _ => '-',
        })
        .collect();
    let joined: Vec<&str> = folded.split('-').filter(|p| !p.is_empty()).take(6).collect();
    if joined.is_empty() {
        "skill".to_string()
    } else {
        joined.join("-")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_croatian_title_becomes_a_typeable_command() {
        assert_eq!(slug("Odobravanje popusta"), "odobravanje-popusta");
        assert_eq!(slug("Izrada ponude za montažu"), "izrada-ponude-za-montazu");
        assert_eq!(slug("Rok plaćanja"), "rok-placanja");
    }

    #[test]
    fn a_title_with_nothing_typeable_still_has_a_command() {
        assert_eq!(slug("—"), "skill");
    }

    #[test]
    fn the_built_in_prompts_all_produce_instructions() {
        for (name, ..) in BUILT_IN {
            let text = built_in_text(name, Some("a quote for ACME"));
            assert!(text.contains("ACME"), "{name} ignored the situation");
            assert!(text.len() > 80, "{name} is too thin to be useful");
        }
    }

    #[test]
    fn checking_against_the_company_always_ends_at_coverage() {
        let text = built_in_text("check-against-company", None);
        assert!(text.contains("check_coverage"));
        assert!(text.contains("get_relevant_context"));
    }

    #[test]
    fn asking_a_price_forbids_estimating_one() {
        let text = built_in_text("what-do-we-charge", Some("montaža"));
        assert!(text.contains("lookup_value"));
        assert!(text.contains("do not estimate"));
    }

    #[test]
    fn a_description_never_runs_away_with_the_whole_body() {
        let body = "# Naslov\n\n".to_string() + &"riječ ".repeat(200);
        assert!(first_line(&body).chars().count() <= 160);
    }
}
