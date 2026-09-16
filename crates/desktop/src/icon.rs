//! The company's own mark, for the chat box.
//!
//! When the owner types a slash in Claude Desktop and sees their own logo
//! next to their own company's procedures, the product stops being "an AI
//! tool we installed" and becomes theirs. That is the entire reason this
//! module exists, and it is worth the eighty lines.
//!
//! The mark is served inline as a `data:` URI rather than as a file path or
//! an HTTP URL. A path would break the moment the client runs in a sandbox
//! that cannot see the owner's home directory, and a URL would mean this
//! local-only product reaches the network to draw an icon.

use std::path::{Path, PathBuf};

use crate::paths;

/// Above this, a logo is refused and the monogram is drawn instead.
///
/// The icon travels inside every `tools/list` reply. A three-megabyte PNG of
/// a company logo would be base64-encoded into four megabytes of JSON on
/// every listing, which is slow in a way nobody would connect to the logo
/// they dropped in a folder.
const MAX_LOGO_BYTES: u64 = 96 * 1024;

/// An icon as the protocol carries it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Icon {
    /// A `data:` URI.
    pub src: String,
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    pub sizes: Vec<String>,
}

/// The company's mark: their own file when they have supplied one, a drawn
/// monogram when they have not.
///
/// Never fails. An icon is a nicety, and a gateway that refused to start
/// because a PNG was malformed would be trading something that matters for
/// something that does not.
pub fn company_icon(company: &str) -> Icon {
    if let Some(icon) = supplied_logo() {
        return icon;
    }
    monogram(company)
}

/// `~/Knowlith/brand/logo.(svg|png)`, when the owner has put one there.
fn supplied_logo() -> Option<Icon> {
    let dir = paths::brand_dir();
    for (name, mime) in [
        ("logo.svg", "image/svg+xml"),
        ("logo.png", "image/png"),
        ("logo.jpg", "image/jpeg"),
        ("logo.jpeg", "image/jpeg"),
        ("logo.webp", "image/webp"),
    ] {
        let path = dir.join(name);
        if let Some(icon) = read_as_icon(&path, mime) {
            return Some(icon);
        }
    }
    None
}

fn read_as_icon(path: &Path, mime: &str) -> Option<Icon> {
    let size = std::fs::metadata(path).ok()?.len();
    if size == 0 || size > MAX_LOGO_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    Some(Icon {
        src: format!("data:{mime};base64,{}", base64(&bytes)),
        mime_type: mime.to_string(),
        // "any" is the honest answer for a file we have not decoded. Claiming
        // 64×64 for an image that is 300×80 makes clients letterbox it.
        sizes: vec!["any".to_string()],
    })
}

/// A drawn mark: the company's initials on a colour derived from its name.
///
/// Deterministic, so the icon does not change between restarts, and distinct
/// enough that two companies in the same industry do not get the same
/// square.
pub fn monogram(company: &str) -> Icon {
    let initials = initials(company);
    let hue = hue_of(company);
    // Held at a fixed saturation and lightness so every generated mark sits
    // in the same visual family and none of them come out neon.
    let background = format!("hsl({hue} 46% 34%)");

    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" role="img" aria-label="{name}">
  <rect width="64" height="64" rx="14" fill="{background}"/>
  <text x="32" y="41" text-anchor="middle" font-family="Helvetica Neue, Helvetica, Arial, sans-serif" font-size="26" font-weight="600" fill="#ffffff">{initials}</text>
</svg>"##,
        name = escape(company),
        background = background,
        initials = escape(&initials),
    );

    Icon {
        src: format!("data:image/svg+xml;base64,{}", base64(svg.as_bytes())),
        mime_type: "image/svg+xml".to_string(),
        sizes: vec!["any".to_string()],
    }
}

/// Where the owner should put their logo, for the interface to tell them.
pub fn logo_location() -> PathBuf {
    paths::brand_dir().join("logo.png")
}

/// Up to two initials, from the words that are actually part of the name.
///
/// Croatian company names end in a legal form — `d.o.o.`, `j.d.o.o.`,
/// `obrt` — that every company in the country shares. Taking the first two
/// words blindly gives half of them the initials "TD".
fn initials(company: &str) -> String {
    const LEGAL: [&str; 7] = ["doo", "jdoo", "dd", "obrt", "ltd", "gmbh", "inc"];

    let words: Vec<&str> = company
        .split(|c: char| c.is_whitespace() || c == '-' || c == '/')
        .filter(|word| {
            let stripped: String = word
                .chars()
                .filter(|c| c.is_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect();
            !stripped.is_empty() && !LEGAL.contains(&stripped.as_str())
        })
        .collect();

    let letters: String = words
        .iter()
        .filter_map(|word| word.chars().find(|c| c.is_alphanumeric()))
        .take(2)
        .flat_map(char::to_uppercase)
        .collect();

    if letters.is_empty() {
        "K".to_string()
    } else {
        letters
    }
}

/// A stable hue in degrees, from the name.
fn hue_of(company: &str) -> u32 {
    let mut hash: u32 = 2166136261;
    for byte in company.to_lowercase().bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(16777619);
    }
    hash % 360
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Standard base64, written out rather than pulled in.
///
/// One dependency's worth of code, and the gateway's dependency list is part
/// of what a company's IT is asked to approve.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);

    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;

        out.push(ALPHABET[(triple >> 18 & 63) as usize] as char);
        out.push(ALPHABET[(triple >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(triple >> 6 & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(triple & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn a_legal_form_is_not_an_initial() {
        assert_eq!(initials("Termoval d.o.o."), "T");
        assert_eq!(initials("Studio Sjever j.d.o.o."), "SS");
        assert_eq!(initials("Pero i sinovi"), "PI");
    }

    #[test]
    fn initials_survive_a_name_with_no_letters() {
        assert_eq!(initials("   "), "K");
        assert_eq!(initials("d.o.o."), "K");
    }

    #[test]
    fn croatian_initials_keep_their_diacritics() {
        assert_eq!(initials("Čakovečki mlinovi"), "ČM");
    }

    #[test]
    fn the_same_company_always_gets_the_same_mark() {
        assert_eq!(monogram("Termoval"), monogram("Termoval"));
        assert_ne!(monogram("Termoval"), monogram("Studio Sjever"));
    }

    #[test]
    fn the_mark_is_a_data_uri_a_client_can_render() {
        let icon = monogram("Termoval d.o.o.");
        assert!(icon.src.starts_with("data:image/svg+xml;base64,"));
        assert_eq!(icon.mime_type, "image/svg+xml");
    }
}
