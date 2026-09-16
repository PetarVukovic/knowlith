//! Live PTY sessions for Claude Code / Codex / Cursor Agent inside the UI.
//!
//! The browser speaks WebSocket; this module owns a real pseudo-terminal on
//! the owner's machine. That is what makes the brain sidebar a *live*
//! terminal rather than a transcript of a command we opened elsewhere.

use std::io::{Read, Write};
use std::sync::Arc;

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
    ws.on_upgrade(handle_socket).into_response()
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum ClientMsg {
    /// Start a CLI session. Only CLI surfaces are accepted.
    Start { app: String, prompt: String },
    /// Raw keystrokes / paste from xterm.
    Input { data: String },
    /// xterm reported a resize.
    Resize { cols: u16, rows: u16 },
}

async fn handle_socket(socket: WebSocket) {
    let (mut sink, mut stream) = socket.split();

    let start = loop {
        match stream.next().await {
            Some(Ok(Message::Text(text))) => match serde_json::from_str::<ClientMsg>(&text) {
                Ok(ClientMsg::Start { app, prompt }) => break (app, prompt),
                Ok(_) => {
                    let _ = sink
                        .send(Message::Text(
                            r#"{"type":"error","message":"send a start frame first"}"#.into(),
                        ))
                        .await;
                }
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
    let Some((binary, args)) = knowlith_desktop::cli_pty_argv(app, &prompt) else {
        let _ = sink
            .send(Message::Text(
                r#"{"type":"error","message":"that assistant has no CLI on this machine"}"#.into(),
            ))
            .await;
        return;
    };

    let command_line = knowlith_desktop::open_with_prompt_opts(app, &prompt, true)
        .command
        .unwrap_or_default();

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
    // Without a colour-capable TERM the CLIs emit plain text — the sidebar
    // looked black-and-white while Terminal.app on the same machine was fine.
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("FORCE_COLOR", "1");
    cmd.env("CLICOLOR_FORCE", "1");
    cmd.env_remove("NO_COLOR");
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
    let mut writer = match pair.master.take_writer() {
        Ok(w) => w,
        Err(e) => {
            let msg = serde_json::to_string(&e.to_string()).unwrap_or_else(|_| "\"writer\"".into());
            let _ = sink
                .send(Message::Text(format!(r#"{{"type":"error","message":{msg}}}"#).into()))
                .await;
            return;
        }
    };

    let master = Arc::new(std::sync::Mutex::new(pair.master));

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
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<ClientMsg>(&text) {
                            Ok(ClientMsg::Input { data }) => {
                                let _ = writer.write_all(data.as_bytes());
                                let _ = writer.flush();
                            }
                            Ok(ClientMsg::Resize { cols, rows }) => {
                                if let Ok(mut m) = master.lock() {
                                    let _ = m.resize(PtySize {
                                        rows: rows.max(8),
                                        cols: cols.max(20),
                                        pixel_width: 0,
                                        pixel_height: 0,
                                    });
                                }
                            }
                            Ok(ClientMsg::Start { .. }) => {}
                            Err(_) => {}
                        }
                    }
                    Some(Ok(Message::Binary(bin))) => {
                        let _ = writer.write_all(&bin);
                        let _ = writer.flush();
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
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
