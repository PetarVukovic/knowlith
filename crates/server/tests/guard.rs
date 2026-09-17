//! The API is not open to whatever page the owner happens to have open.
//!
//! These are regression tests for three real holes, all from the same
//! wrong assumption — that binding to `127.0.0.1` made the daemon private.
//! It made it unreachable from the network. Any website the owner visits
//! can still ask the browser to send a request here.
//!
//! Each test below names the attack rather than the mechanism, because the
//! mechanism is allowed to change and the attack is not allowed to work.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use knowlith_lake::Lake;
use knowlith_server::{AppState, auth::Token, router};
use tower::ServiceExt;

const SECRET: &str = "the-owners-token";

fn app() -> axum::Router {
    // A lake of its own per test, so nothing here depends on the order
    // these run in or on what is in the developer's own `~/Knowlith`.
    // A counter, not only a timestamp: two tests on two threads read the
    // same nanosecond, shared one lake file, and the second one's job was
    // silently swallowed by the idempotency key the first had already
    // used. That failed about one run in ten and looked like a queue bug.
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "knowlith-guard-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).expect("a directory");
    let lake = Lake::open(&dir.join("lake.sqlite")).expect("a lake");
    let state = AppState::with_token(lake, "Test Company", Token::from_value(SECRET));
    router(state)
}

async fn status(request: Request<Body>) -> StatusCode {
    app().oneshot(request).await.expect("a response").status()
}

/// A request with no token at all — what a page on another origin can send
/// without the browser asking anybody's permission first.
fn without_token(method: &str, path: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .body(Body::empty())
        .expect("a request")
}

fn with_token(method: &str, path: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header(knowlith_server::auth::HEADER, token)
        .body(Body::empty())
        .expect("a request")
}

#[tokio::test]
async fn a_website_cannot_enumerate_the_owners_disk() {
    // `GET /api/sources/preview?path=…` reports how many files a folder
    // holds, of what kinds, and when it was last touched. Answered to
    // anyone, that is a map of the whole machine.
    let path = "/api/sources/preview?path=%2Fetc";
    assert_eq!(status(without_token("GET", path)).await, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_website_cannot_make_the_daemon_read_a_folder() {
    // The worst of the three: `POST /api/sources` queues a walk of any
    // directory, extracts the text in it into the lake, and `GET
    // /api/objects` then reads it back out.
    let request = Request::builder()
        .method("POST")
        .uri("/api/sources")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"path":"/etc"}"#))
        .expect("a request");
    assert_eq!(status(request).await, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_website_cannot_open_a_dialog_on_the_owners_desktop() {
    // This one carries no body, so no browser would have asked us first.
    // If the guard let it through, a page could pop a native folder
    // chooser on the owner's screen at will.
    assert_eq!(
        status(without_token("POST", "/api/sources/browse")).await,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn reading_and_writing_are_both_behind_the_token() {
    // Not only the endpoints that touch the disk. The lake is the
    // company's own knowledge, and approving something is an act.
    for (method, path) in [
        ("GET", "/api/health"),
        ("GET", "/api/objects"),
        ("GET", "/api/usage"),
        ("GET", "/api/activity"),
        ("GET", "/api/tools"),
        ("PUT", "/api/company"),
        ("POST", "/api/tools/codex/connect"),
    ] {
        assert_eq!(
            status(without_token(method, path)).await,
            StatusCode::UNAUTHORIZED,
            "{method} {path} answered without a token"
        );
    }
}

#[tokio::test]
async fn a_wrong_token_is_no_better_than_none() {
    assert_eq!(
        status(with_token("GET", "/api/health", "the-owners-toke")).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        status(with_token("GET", "/api/health", "")).await,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn the_owners_own_interface_is_served_as_before() {
    assert_eq!(status(with_token("GET", "/api/health", SECRET)).await, StatusCode::OK);
}

#[tokio::test]
async fn a_missing_endpoint_says_so_rather_than_blaming_the_token() {
    // `route_layer`, not `layer`. Reported as 401, a typo in a path sends
    // whoever is debugging it hunting a permission problem that is not
    // there.
    assert_eq!(
        status(without_token("GET", "/api/nothing-here")).await,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn the_page_itself_needs_no_token() {
    // It is where the token comes from. Without this the interface could
    // never load, and the owner would see nothing at all.
    let status = status(without_token("GET", "/")).await;
    assert_ne!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_rebound_domain_is_not_handed_the_page_or_the_token() {
    // DNS rebinding: a page loaded from `evil.example.com:7717` whose DNS
    // answer is then switched to 127.0.0.1 is same-origin with the daemon
    // as far as the browser knows, so it may read `/` — and the token in
    // it. The only thing left that tells the two apart is `Host`.
    for path in ["/", "/index.html", "/api/health"] {
        let response = app()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(path)
                    .header("host", "evil.example.com:7717")
                    .header(knowlith_server::auth::HEADER, SECRET)
                    .body(Body::empty())
                    .expect("a request"),
            )
            .await
            .expect("a response");
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{path} answered a rebound host");
        let body = axum::body::to_bytes(response.into_body(), 1 << 20).await.expect("a body");
        assert!(
            !String::from_utf8_lossy(&body).contains(SECRET),
            "{path} leaked the token to a rebound host"
        );
    }
}

#[tokio::test]
async fn the_owners_own_address_is_still_served() {
    // The check above must not turn away the addresses the owner really
    // types, or the ones Vite and the shipped binary answer at.
    for host in ["127.0.0.1:7717", "localhost:5173", "localhost", "[::1]:7717"] {
        let response = app()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/health")
                    .header("host", host)
                    .header(knowlith_server::auth::HEADER, SECRET)
                    .body(Body::empty())
                    .expect("a request"),
            )
            .await
            .expect("a response");
        assert_eq!(response.status(), StatusCode::OK, "{host} was refused");
    }
}

#[tokio::test]
async fn nothing_here_is_offered_to_another_origin() {
    // The hole was `allow_origin(Any)`: with it, a page anywhere could not
    // only trigger these endpoints but read what came back. No response
    // from this router may carry that header, the page included — the page
    // is what the token is written into.
    for path in ["/", "/api/health"] {
        let response = app()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(path)
                    .header("origin", "https://example.invalid")
                    .header(knowlith_server::auth::HEADER, SECRET)
                    .body(Body::empty())
                    .expect("a request"),
            )
            .await
            .expect("a response");
        assert!(
            response.headers().get("access-control-allow-origin").is_none(),
            "{path} offered itself to another origin"
        );
    }
}

#[tokio::test]
async fn a_token_in_the_url_is_never_enough() {
    // A secret in a URL is copied into chats, screenshots and history, so no
    // endpoint may treat `?token=` as authentication.
    assert_eq!(
        status(without_token("GET", &format!("/api/health?token={SECRET}"))).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        status(without_token("GET", &format!("/api/brain?token={SECRET}"))).await,
        StatusCode::UNAUTHORIZED
    );
}
