//! Lossless UTF-8 chunks, with no external process or tokenizer startup.

pub fn chunk_text(text: &str, chunk_size: usize) -> Vec<String> {
    let limit = chunk_size.max(4);
    let mut rest = text;
    let mut chunks = Vec::new();
    while !rest.is_empty() {
        let mut end = rest.len().min(limit);
        while !rest.is_char_boundary(end) { end -= 1; }
        // Prefer a nearby paragraph boundary without dropping whitespace.
        if end < rest.len() {
            if let Some(boundary) = rest[..end].rfind("\n\n") {
                if boundary > end / 2 { end = boundary + 2; }
            }
        }
        chunks.push(rest[..end].to_string());
        rest = &rest[end..];
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn long_unicode_paragraphs_are_bounded_and_lossless() {
        let text = "Ž🙂 abc ".repeat(2000);
        let chunks = chunk_text(&text, 1800);
        assert!(chunks.iter().all(|c| c.len() <= 1800));
        assert_eq!(chunks.concat(), text);
    }
}
