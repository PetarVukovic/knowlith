//! Who is allowed to talk to the daemon.
//!
//! The API listens on loopback, and the router used to claim that was
//! enough. It is not, and the mistake is a common one: loopback keeps the
//! *network* out, but it does nothing about the owner's own browser, which
//! will send a request to `127.0.0.1` on behalf of whatever page they
//! happen to have open. Without a check here, any website could ask this
//! daemon to walk a folder on the disk and then read back what was in it.
//!
//! So every `/api` request must carry a secret this process wrote to a file
//! only the owner can read. That defends twice over. A page cannot read the
//! file, so it cannot know the value; and setting a header at all is
//! something the browser will not do cross-origin without asking us first,
//! which we never allow.
//!
//! The token lives on disk rather than only in memory so that restarting
//! the daemon does not break the tab the owner already has open, and so the
//! development server can pick it up without either half knowing about the
//! other.

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

/// The header the interface sends on every request.
///
/// A custom header, deliberately. A cross-origin request that carries one
/// is never "simple" — the browser asks permission first, and this daemon
/// grants none.
pub const HEADER: &str = "x-knowlith-token";

/// Where the secret is kept.
pub fn file() -> PathBuf {
    knowlith_desktop::paths::root().join("api.token")
}

/// The secret that separates the owner's interface from every other page.
#[derive(Clone)]
pub struct Token(Arc<str>);

// Printed by mistake is how secrets end up in logs and screenshots, and a
// derived `Debug` on a struct three levels up is enough to do it.
impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(hidden)")
    }
}

impl Token {
    /// This machine's token, written the first time it is asked for.
    ///
    /// A token that cannot be written to disk is still a token: the daemon
    /// keeps it in memory and carries on. That costs the development server
    /// its copy, which is a broken workbench — but running without a token
    /// at all would be a folder-reading daemon open to any page, and that
    /// is not a trade worth making silently.
    pub fn load() -> Token {
        let path = file();
        if let Ok(existing) = std::fs::read_to_string(&path) {
            let existing = existing.trim();
            if !existing.is_empty() {
                return Token(existing.into());
            }
        }

        let fresh = fresh();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = write_private(&path, &fresh) {
            eprintln!("Knowlith could not write {}: {e}", path.display());
        }
        Token(fresh.into())
    }

    /// A token with a known value, for tests and for anything that needs to
    /// hand the same secret to two processes.
    pub fn from_value(value: impl Into<Arc<str>>) -> Token {
        Token(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether a request carried the right secret.
    ///
    /// Compared in constant time. The margin this buys against a local
    /// attacker timing a loopback socket is thin, but the alternative is
    /// writing `==` and having to argue about how thin.
    pub fn matches(&self, given: &str) -> bool {
        same(self.0.as_bytes(), given.as_bytes())
    }
}

/// 32 bytes from the operating system, in hex.
///
/// A failure here is not recoverable and must not be papered over: a
/// predictable token is worse than a daemon that refuses to start, because
/// it looks exactly like a working one.
fn fresh() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the operating system's random source is unavailable");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Writes the token so that only this user can read it.
///
/// The permission is set as the file is created rather than afterwards,
/// because between the two there is a moment where it is world-readable.
fn write_private(path: &std::path::Path, value: &str) -> std::io::Result<()> {
    use std::io::Write;

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut handle = options.open(path)?;
    handle.write_all(value.as_bytes())?;
    handle.write_all(b"\n")
}

fn same(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |seen, (x, y)| seen | (x ^ y)) == 0
}

/// Puts the token where the interface can find it.
///
/// The page the daemon serves is same-origin with the API, so it could be
/// trusted with the secret by any number of routes; a meta tag is the one
/// that survives the page being a static file compiled into the binary.
///
/// Nothing escapes the value because nothing can: it is hex, checked by a
/// test below.
pub fn inject(page: &[u8], token: &Token) -> Vec<u8> {
    let tag = format!(r#"<meta name="knowlith-token" content="{}">"#, token.as_str());
    let Ok(html) = std::str::from_utf8(page) else {
        return page.to_vec();
    };
    match html.find("<head>") {
        Some(at) => {
            let (before, after) = html.split_at(at + "<head>".len());
            format!("{before}{tag}{after}").into_bytes()
        }
        // No head is not a page we built, but a browser will still read a
        // meta tag it finds first, so the interface keeps working.
        None => format!("{tag}{html}").into_bytes(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_token_is_long_and_hexadecimal() {
        let token = fresh();
        assert_eq!(token.len(), 64);
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
        // Which is what lets `inject` skip escaping.
        assert!(!token.contains(['<', '>', '"', '&']));
    }

    #[test]
    fn two_tokens_are_never_the_same() {
        assert_ne!(fresh(), fresh());
    }

    #[test]
    fn only_the_exact_secret_matches() {
        let token = Token::from_value("abc123");
        assert!(token.matches("abc123"));
        assert!(!token.matches("abc124"));
        assert!(!token.matches("abc12"));
        assert!(!token.matches("abc1234"));
        assert!(!token.matches(""));
    }

    #[test]
    fn the_secret_is_not_printed_by_accident() {
        let shown = format!("{:?}", Token::from_value("hunter2"));
        assert!(!shown.contains("hunter2"), "{shown}");
    }

    #[test]
    fn the_page_carries_the_token_where_the_interface_looks_for_it() {
        let page = b"<!doctype html><html><head><title>Knowlith</title></head><body></body></html>";
        let out = inject(page, &Token::from_value("secret"));
        let out = String::from_utf8(out).unwrap();
        assert!(out.contains(r#"<meta name="knowlith-token" content="secret">"#), "{out}");
        // Inside the head, and before anything else in it.
        assert!(out.contains(r#"<head><meta name="knowlith-token""#), "{out}");
        assert!(out.contains("<title>Knowlith</title>"), "{out}");
    }

    #[test]
    fn a_page_without_a_head_still_gets_the_token() {
        let out = inject(b"<p>hello</p>", &Token::from_value("secret"));
        let out = String::from_utf8(out).unwrap();
        assert!(out.starts_with(r#"<meta name="knowlith-token" content="secret">"#), "{out}");
        assert!(out.ends_with("<p>hello</p>"), "{out}");
    }
}
