//! What the owner's CLI actually printed about tokens and price.
//!
//! The compiler never invents a cost. A silent CLI is stored as silence,
//! not as `$0`. A price list in this crate would be a second source of
//! truth that drifts from the bill the provider already sent.

use serde_json::Value;

/// Tokens and cost one CLI invoke reported, if it reported any.
///
/// Every field is optional because the three engines do not print the
/// same envelope, and Cursor often prints none. Missing is missing —
/// it is not zero.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EngineUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_tokens: Option<u64>,
    pub cost_usd: Option<f64>,
    pub model: Option<String>,
}

impl EngineUsage {
    pub fn is_silent(&self) -> bool {
        self.input_tokens.is_none()
            && self.output_tokens.is_none()
            && self.cache_tokens.is_none()
            && self.cost_usd.is_none()
    }

    /// The figures the owner is owed as one number, when any part was given.
    pub fn total_tokens(&self) -> Option<u64> {
        let parts = [self.input_tokens, self.output_tokens, self.cache_tokens];
        if parts.iter().all(Option::is_none) {
            return None;
        }
        Some(parts.iter().map(|part| part.unwrap_or(0)).sum())
    }

    /// Last reported figure wins. One CLI invoke prints running totals
    /// across JSONL events; summing them would bill the same tokens twice.
    pub fn overwrite(&mut self, other: Self) {
        if other.input_tokens.is_some() {
            self.input_tokens = other.input_tokens;
        }
        if other.output_tokens.is_some() {
            self.output_tokens = other.output_tokens;
        }
        if other.cache_tokens.is_some() {
            self.cache_tokens = other.cache_tokens;
        }
        if other.cost_usd.is_some() {
            self.cost_usd = other.cost_usd;
        }
        if other.model.is_some() {
            self.model = other.model;
        }
    }

    /// Separate invokes add. Drafting three skills is three bills.
    pub fn add_invoke(&mut self, other: Self) {
        self.input_tokens = add_u64(self.input_tokens, other.input_tokens);
        self.output_tokens = add_u64(self.output_tokens, other.output_tokens);
        self.cache_tokens = add_u64(self.cache_tokens, other.cache_tokens);
        self.cost_usd = add_f64(self.cost_usd, other.cost_usd);
        if self.model.is_none() {
            self.model = other.model;
        } else if other.model.is_some() && self.model != other.model {
            // Two models in one job: naming one of them would be a lie.
            self.model = None;
        }
    }

    pub fn fold_invoke(acc: Option<Self>, next: Option<Self>) -> Option<Self> {
        match (acc, next.filter(|usage| !usage.is_silent())) {
            (None, next) => next,
            (acc, None) => acc,
            (Some(mut acc), Some(next)) => {
                acc.add_invoke(next);
                Some(acc)
            }
        }
    }

    /// Owner-facing, in English, with only the parts the CLI filled in.
    ///
    /// `Codex · 4,218 tokens · $0.04`. A silent usage is just the engine
    /// name, and callers that have nothing to show should not call this.
    pub fn phrase(&self, engine: &str) -> String {
        let mut bits = vec![engine.to_string()];
        if let Some(n) = self.total_tokens() {
            bits.push(token_words(n));
        }
        if let Some(cost) = self.cost_usd {
            bits.push(usd(cost));
        }
        bits.join(" · ")
    }
}

fn add_u64(a: Option<u64>, b: Option<u64>) -> Option<u64> {
    match (a, b) {
        (None, None) => None,
        (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)),
    }
}

fn add_f64(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (None, None) => None,
        (a, b) => Some(a.unwrap_or(0.0) + b.unwrap_or(0.0)),
    }
}

fn token_words(n: u64) -> String {
    if n == 1 {
        "1 token".into()
    } else {
        format!("{} tokens", group_u64(n))
    }
}

fn group_u64(n: u64) -> String {
    let digits: Vec<char> = n.to_string().chars().collect();
    let mut out = String::new();
    for (i, ch) in digits.iter().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*ch);
    }
    out
}

fn usd(cost: f64) -> String {
    if cost == 0.0 {
        "$0".into()
    } else if cost.abs() < 0.01 {
        format!("${cost:.4}")
    } else {
        format!("${cost:.2}")
    }
}

/// Claude Code / Cursor Agent: `--output-format json` wraps the compile
/// payload in `result` and usually carries `usage` / `total_cost_usd`.
pub fn parse_json_reply(stdout: &str, stderr: &str) -> (String, Option<EngineUsage>) {
    let mut text = String::new();
    let usage = scrape_usage(stdout, stderr, &mut text);
    if text.is_empty() {
        text = stdout.trim().to_string();
    }
    (text, usage)
}

/// Codex keeps the answer in `--output-last-message`. Stdout is progress
/// JSONL; this walks it for token counts and never treats it as the answer.
pub fn scrape_usage_only(stdout: &str, stderr: &str) -> Option<EngineUsage> {
    let mut ignored = String::new();
    scrape_usage(stdout, stderr, &mut ignored)
}

fn scrape_usage(stdout: &str, stderr: &str, text: &mut String) -> Option<EngineUsage> {
    let mut usage = EngineUsage::default();
    if let Ok(value) = serde_json::from_str::<Value>(stdout.trim()) {
        take_result(&value, text);
        usage.overwrite(from_value(&value));
    }
    for line in stdout.lines().chain(stderr.lines()) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        take_result(&value, text);
        usage.overwrite(from_value(&value));
    }
    if usage.is_silent() {
        None
    } else {
        Some(usage)
    }
}

fn take_result(value: &Value, text: &mut String) {
    let Some(extracted) = result_text(value) else {
        return;
    };
    if !extracted.is_empty() {
        *text = extracted;
    }
}

fn result_text(value: &Value) -> Option<String> {
    match value.get("result") {
        Some(Value::String(s)) => Some(s.clone()),
        Some(other) if other.is_object() || other.is_array() => Some(other.to_string()),
        _ => None,
    }
}

fn from_value(value: &Value) -> EngineUsage {
    let mut usage = EngineUsage::default();
    fill(&mut usage, value);
    if let Some(nested) = value.get("usage") {
        fill(&mut usage, nested);
    }
    if let Some(info) = value.get("info") {
        fill(&mut usage, info);
        if let Some(total) = info.get("total_token_usage") {
            fill(&mut usage, total);
        } else if let Some(last) = info.get("last_token_usage") {
            fill(&mut usage, last);
        }
    }
    if let Some(total) = value.get("total_token_usage") {
        fill(&mut usage, total);
    } else if let Some(last) = value.get("last_token_usage") {
        fill(&mut usage, last);
    }
    if let Some(payload) = value.get("payload") {
        usage.overwrite(from_value(payload));
    }
    usage
}

fn fill(usage: &mut EngineUsage, value: &Value) {
    if !value.is_object() {
        return;
    }
    if let Some(n) = take_u64(value, &["input_tokens", "prompt_tokens", "inputTokens"]) {
        usage.input_tokens = Some(n);
    }
    if let Some(n) = take_u64(value, &["output_tokens", "completion_tokens", "outputTokens"]) {
        usage.output_tokens = Some(n);
    }
    let cache = [
        "cache_tokens",
        "cached_input_tokens",
        "cache_read_input_tokens",
        "cache_creation_input_tokens",
        "cache_creation_tokens",
    ];
    let mut cache_total: Option<u64> = None;
    for key in cache {
        if let Some(n) = take_u64(value, &[key]) {
            cache_total = Some(cache_total.unwrap_or(0) + n);
        }
    }
    if let Some(n) = cache_total {
        usage.cache_tokens = Some(n);
    }
    if let Some(cost) = take_f64(value, &["total_cost_usd", "cost_usd"]) {
        usage.cost_usd = Some(cost);
    }
    if let Some(model) = value.get("model").and_then(Value::as_str) {
        if model.len() > 2 {
            usage.model = Some(model.to_string());
        }
    }
}

fn take_u64(value: &Value, keys: &[&str]) -> Option<u64> {
    for key in keys {
        if let Some(found) = value.get(*key).and_then(as_u64) {
            return Some(found);
        }
    }
    None
}

fn take_f64(value: &Value, keys: &[&str]) -> Option<f64> {
    for key in keys {
        if let Some(found) = value.get(*key).and_then(as_f64) {
            return Some(found);
        }
    }
    None
}

fn as_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|n| u64::try_from(n).ok()))
        .or_else(|| {
            value.as_f64().and_then(|n| {
                if n >= 0.0 && n.fract() == 0.0 && n <= u64::MAX as f64 {
                    Some(n as u64)
                } else {
                    None
                }
            })
        })
}

fn as_f64(value: &Value) -> Option<f64> {
    value.as_f64().or_else(|| value.as_i64().map(|n| n as f64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_json_is_the_source_of_the_answer_and_the_bill() {
        let stdout = r#"{
            "type": "result",
            "result": "{\"candidates\":[]}",
            "usage": {"input_tokens": 4000, "output_tokens": 218, "cache_read_input_tokens": 10},
            "total_cost_usd": 0.04,
            "model": "claude-sonnet-4-5"
        }"#;
        let (text, usage) = parse_json_reply(stdout, "");
        assert_eq!(text, "{\"candidates\":[]}");
        let usage = usage.expect("claude reported usage");
        assert_eq!(usage.input_tokens, Some(4000));
        assert_eq!(usage.output_tokens, Some(218));
        assert_eq!(usage.cache_tokens, Some(10));
        assert_eq!(usage.cost_usd, Some(0.04));
        assert_eq!(usage.model.as_deref(), Some("claude-sonnet-4-5"));
        assert_eq!(usage.phrase("Claude Code"), "Claude Code · 4,228 tokens · $0.04");
    }

    #[test]
    fn a_result_that_is_already_an_object_is_still_the_compile_payload() {
        let stdout = r#"{"type":"result","result":{"candidates":[]},"usage":{"input_tokens":1,"output_tokens":2}}"#;
        let (text, usage) = parse_json_reply(stdout, "");
        assert!(text.contains("\"candidates\""));
        assert_eq!(usage.unwrap().total_tokens(), Some(3));
    }

    #[test]
    fn a_silent_cli_is_not_priced_at_zero() {
        let (text, usage) = parse_json_reply(r#"{"candidates":[]}"#, "");
        assert_eq!(text, r#"{"candidates":[]}"#);
        assert!(usage.is_none(), "missing usage is not $0");
    }

    #[test]
    fn compile_json_is_not_mistaken_for_a_token_count() {
        let stdout = r#"{"candidates":[{"kind":"rule","title":"Rok","statement":"15.","quotes":["15"]}]}"#;
        let (_, usage) = parse_json_reply(stdout, "");
        assert!(usage.is_none());
    }

    #[test]
    fn codex_jsonl_token_count_does_not_become_the_answer() {
        let stdout = concat!(
            "{\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",",
            "\"info\":{\"total_token_usage\":{\"input_tokens\":50,\"cached_input_tokens\":4,\"output_tokens\":7}}}}\n",
            "{\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",",
            "\"info\":{\"total_token_usage\":{\"input_tokens\":80,\"cached_input_tokens\":4,\"output_tokens\":20}}}}\n"
        );
        let usage = scrape_usage_only(stdout, "").expect("codex printed tokens");
        assert_eq!(usage.input_tokens, Some(80), "the last total wins, not the sum");
        assert_eq!(usage.output_tokens, Some(20));
        assert_eq!(usage.cache_tokens, Some(4));
        assert!(usage.cost_usd.is_none(), "codex did not print a dollar figure");
        assert_eq!(usage.phrase("Codex"), "Codex · 104 tokens");
    }

    #[test]
    fn one_token_is_not_pluralised() {
        let usage = EngineUsage {
            input_tokens: Some(1),
            ..EngineUsage::default()
        };
        assert_eq!(usage.phrase("Codex"), "Codex · 1 token");
    }

    #[test]
    fn a_sub_cent_cost_keeps_its_digits() {
        let usage = EngineUsage {
            cost_usd: Some(0.0023),
            ..EngineUsage::default()
        };
        assert_eq!(usage.phrase("Claude Code"), "Claude Code · $0.0023");
    }

    #[test]
    fn separate_invokes_add_and_a_silent_one_does_not_count() {
        let first = EngineUsage {
            input_tokens: Some(10),
            cost_usd: Some(0.01),
            ..EngineUsage::default()
        };
        let silent = EngineUsage::default();
        let second = EngineUsage {
            input_tokens: Some(5),
            cost_usd: Some(0.02),
            ..EngineUsage::default()
        };
        let merged = EngineUsage::fold_invoke(Some(first), Some(silent));
        let merged = EngineUsage::fold_invoke(merged, Some(second)).unwrap();
        assert_eq!(merged.input_tokens, Some(15));
        assert_eq!(merged.cost_usd, Some(0.03));
    }
}
