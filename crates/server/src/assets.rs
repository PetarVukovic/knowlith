//! The interface, served from inside the binary.
//!
//! A single-page application has one rule that a plain static file server
//! gets wrong: a request for `/review/rule:sales.discount` is not a missing
//! file, it is the same `index.html` with a different path in the address
//! bar. Returning 404 there is why a refresh breaks an SPA, and a refresh is
//! the first thing anyone does when something looks stale.

include!(concat!(env!("OUT_DIR"), "/assets.rs"));

/// Whether an interface was compiled in at all.
pub fn present() -> bool {
    !ASSETS.is_empty()
}

/// The bytes and content type for a request path.
pub fn file(path: &str) -> Option<(&'static [u8], &'static str)> {
    let wanted = path.trim_start_matches('/');
    let wanted = if wanted.is_empty() { "index.html" } else { wanted };

    ASSETS
        .iter()
        .find(|(route, _)| *route == wanted)
        .map(|(route, bytes)| (*bytes, content_type(route)))
}

/// The page itself, for any route the application handles in the browser.
pub fn index() -> Option<&'static [u8]> {
    ASSETS
        .iter()
        .find(|(route, _)| *route == "index.html")
        .map(|(_, bytes)| *bytes)
}

/// Types by extension.
///
/// A wrong type here is not cosmetic: a stylesheet served as `text/plain`
/// is ignored by every browser, and the interface renders as unstyled text
/// with no error anywhere to explain it.
fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "map" => "application/json",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stylesheet_is_never_served_as_plain_text() {
        assert_eq!(content_type("assets/index-abc.css"), "text/css; charset=utf-8");
        assert_eq!(content_type("assets/index-abc.js"), "text/javascript; charset=utf-8");
        assert!(content_type("index.html").starts_with("text/html"));
    }

    #[test]
    fn an_unknown_extension_is_downloaded_rather_than_rendered() {
        assert_eq!(content_type("company.sqlite"), "application/octet-stream");
        assert_eq!(content_type("noextension"), "application/octet-stream");
    }

    #[test]
    fn the_root_means_the_page() {
        // Whether an interface is compiled in or not, asking for "/" must
        // resolve to the same route as asking for "/index.html".
        assert_eq!(file("/").is_some(), file("/index.html").is_some());
    }
}
