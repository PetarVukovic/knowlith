//! Headless CLI runs for the company chat.
//!
//! The browser speaks WebSocket; this module runs the owner's own Claude /
//! Codex / Cursor CLI on a pseudo-terminal on the owner's machine, in print
//! mode, and streams what it writes. The PTY is there because the CLIs
//! behave differently on a pipe (buffering, no MCP status), not because
//! anything on the page is a terminal — the page shows bubbles.

use std::io::Read;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Query, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use serde::Deserialize;

use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct TerminalQuery {
    /// Optional: browsers cannot set custom headers on WebSocket, so the
    /// shipped page passes the token here. Vite's proxy attaches the header
    /// in development instead.
    #[serde(default)]
    pub token: Option<String>,
}

/// `GET /api/terminal` — upgrades to a PTY WebSocket.
pub async fn terminal_ws(
    State(state): State<AppState>,
    Query(query): Query<TerminalQuery>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    // Guard already checked the header; query token covers the production
    // page where the browser opens the socket without that header.
    if let Some(token) = query.token.as_deref().filter(|t| !t.is_empty()) {
        if !state.token.matches(token) {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
    }
    let server_key = knowlith_desktop::server_key(&state.company);
    ws.on_upgrade(move |socket| handle_socket(socket, server_key))
        .into_response()
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum ClientMsg {
    /// Ask one question of one CLI. Only CLI surfaces are accepted.
    Start { app: String, prompt: String },
}

async fn handle_socket(socket: WebSocket, server_key: String) {
    let (mut sink, mut stream) = socket.split();

    let start = loop {
        match stream.next().await {
            Some(Ok(Message::Text(text))) => match serde_json::from_str::<ClientMsg>(&text) {
                Ok(ClientMsg::Start { app, prompt }) => break (app, prompt),
                Err(e) => {
                    let msg = serde_json::to_string(&e.to_string()).unwrap_or_else(|_| "\"bad json\"".into());
                    let _ = sink
                        .send(Message::Text(format!(r#"{{"type":"error","message":{msg}}}"#).into()))
                        .await;
                    return;
                }
            },
            Some(Ok(Message::Close(_))) | None => return,
            Some(Ok(_)) => continue,
            Some(Err(_)) => return,
        }
    };

    let (app_slug, prompt) = start;
    let Some(app) = knowlith_desktop::App::parse(&app_slug) else {
        let _ = sink
            .send(Message::Text(
                format!(r#"{{"type":"error","message":"unknown app {app_slug}"}}"#).into(),
            ))
            .await;
        return;
    };
    let Some((binary, args)) = knowlith_desktop::cli_print_argv(app, &server_key, &prompt) else {
        let _ = sink
            .send(Message::Text(
                r#"{"type":"error","message":"that assistant has no CLI on this machine"}"#.into(),
            ))
            .await;
        return;
    };

    let command_line = knowlith_desktop::cli_command_line(app, &prompt).unwrap_or_default();

    let pty_system = native_pty_system();
    let pair = match pty_system.openpty(PtySize {
        rows: 28,
        cols: 100,
        pixel_width: 0,
        pixel_height: 0,
    }) {
        Ok(p) => p,
        Err(e) => {
            let msg = serde_json::to_string(&e.to_string()).unwrap_or_else(|_| "\"pty\"".into());
            let _ = sink
                .send(Message::Text(format!(r#"{{"type":"error","message":{msg}}}"#).into()))
                .await;
            return;
        }
    };

    let mut cmd = CommandBuilder::new(&binary);
    for arg in &args {
        cmd.arg(arg);
    }
    // Chat wants plain prose. Colour + a capable TERM makes CLIs paint
    // spinners and boxes into the reply.
    cmd.env("TERM", "dumb");
    cmd.env("NO_COLOR", "1");
    cmd.env_remove("FORCE_COLOR");
    cmd.env_remove("COLORTERM");
    cmd.env_remove("CLICOLOR_FORCE");
    if let Some(home) = std::env::var_os("HOME") {
        cmd.cwd(home);
    }

    let mut child = match pair.slave.spawn_command(cmd) {
        Ok(c) => c,
        Err(e) => {
            let msg = serde_json::to_string(&e.to_string()).unwrap_or_else(|_| "\"spawn\"".into());
            let _ = sink
                .send(Message::Text(format!(r#"{{"type":"error","message":{msg}}}"#).into()))
                .await;
            return;
        }
    };

    let mut reader = match pair.master.try_clone_reader() {
        Ok(r) => r,
        Err(e) => {
            let msg = serde_json::to_string(&e.to_string()).unwrap_or_else(|_| "\"reader\"".into());
            let _ = sink
                .send(Message::Text(format!(r#"{{"type":"error","message":{msg}}}"#).into()))
                .await;
            return;
        }
    };
    // Kept alive until the child exits: dropping the master closes the
    // child's terminal under it.
    let _master = pair.master;

    let ready = serde_json::json!({
        "type": "ready",
        "label": app.label(),
        "command": command_line,
    });
    let _ = sink.send(Message::Text(ready.to_string().into())).await;

    let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if out_tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    loop {
        tokio::select! {
            incoming = stream.next() => {
                // Nothing goes to the child after the question: the run is
                // one prompt, one answer. Closing the socket kills it, which
                // is the chat's Stop button.
                match incoming {
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    Some(Ok(_)) => {}
                }
            }
            chunk = out_rx.recv() => {
                match chunk {
                    Some(bytes) => {
                        if sink.send(Message::Binary(bytes.into())).await.is_err() {
                            break;
                        }
                    }
                    None => {
                        let _ = sink.send(Message::Text(r#"{"type":"exit"}"#.into())).await;
                        break;
                    }
                }
            }
        }
    }

    let _ = child.kill();
}
