//! Writing Knowlith into someone else's configuration file, safely.
//!
//! This module edits files the owner did not create and may have edited by
//! hand. Three rules follow from that, and they are the whole design:
//!
//! 1. **Never rewrite what we did not put there.** The file is parsed, one
//!    key is set, and the rest is written back as it was. For TOML that means
//!    `toml_edit`, which preserves comments and ordering — a Codex user's
//!    commented-out server has to still be there afterwards.
//! 2. **Never lose the previous version.** Every change writes a timestamped
//!    backup next to the file first. Recovery is then a rename the owner can
//!    do themselves, without us.
//! 3. **Never claim more than was verified.** The file is written through a
//!    temporary file and a rename, then read back and re-parsed, and only
//!    then is the connection reported as made.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use crate::apps::{App, Format};
use crate::paths;

/// The key used before connectors listed the company by name.
///
/// Still recognised on read and removed on write, so an older install that
/// says `knowlith` (or a bare company name from a brief interim) becomes
/// `knowlith-<company>` the next time Connect is pressed — without asking
/// the owner to edit JSON by hand.
pub const LEGACY_SERVER_NAME: &str = "knowlith";

/// Kept so older callers compile; prefer [`server_key`].
pub const SERVER_NAME: &str = LEGACY_SERVER_NAME;

/// What Claude / Cursor / Codex show in their connectors list.
///
/// The list label *is* the `mcpServers` key. Form is `knowlith-<company>`
/// so the product stays recognisable and two companies on one machine do
/// not collide. Empty / placeholder names fall back to the legacy product
/// key rather than inventing a label the owner did not choose.
pub fn server_key(company: &str) -> String {
    let trimmed = company.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("your company") {
        return LEGACY_SERVER_NAME.to_string();
    }
    format!("knowlith-{}", company_slug(trimmed))
}

/// Company name as a connector-safe suffix: lowercase, no diacritics, no
/// spaces. `Termoval d.o.o.` → `termoval-d-o-o`.
fn company_slug(company: &str) -> String {
    let folded: String = company
        .chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| match c {
            'č' | 'ć' => 'c',
            'ž' => 'z',
            'š' => 's',
            'đ' => 'd',
            c if c.is_ascii_alphanumeric() => c,
            _ => '-',
        })
        .collect();
    let joined: Vec<&str> = folded.split('-').filter(|p| !p.is_empty()).take(8).collect();
    if joined.is_empty() {
        "company".to_string()
    } else {
        joined.join("-")
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    #[error("{0} does not keep its settings anywhere this system knows about")]
    NoConfigLocation(&'static str),
    #[error("could not read {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("could not write {path}: {source}")]
    Write {
        path: String,
        #[source]
        source: std::io::Error,
    },
    /// The file exists and is not what it claims to be. The owner is told
    /// where the backup went, because the alternative is a tool that silently
    /// deletes a config someone spent an afternoon on.
    #[error("{path} is not valid {format} and was left alone; a copy is at {backup}")]
    Unreadable {
        path: String,
        format: &'static str,
        backup: String,
    },
}

type Result<T> = std::result::Result<T, ConnectError>;

/// What an application's configuration says about Knowlith right now.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub app: App,
    pub label: &'static str,
    pub slug: &'static str,
    pub installed: bool,
    /// Where the answer came from, so an owner who wants to check can.
    pub config_path: Option<String>,
    pub connected: bool,
    /// The key currently sitting in the client's server list, when any —
    /// usually the company name, or `knowlith` on an older install.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_key: Option<String>,
    /// Set when the config names a `knowlith` binary that is not this one.
    /// After an upgrade that moves the binary, the entry is still there and
    /// still says "connected" while pointing at a file that no longer
    /// exists — which looks exactly like a broken product.
    pub stale_command: Option<String>,
    pub needs_restart: bool,
    pub refresh_hint: &'static str,
    /// Whether the application is running, so the interface can say
    /// "restart it" rather than "open it".
    pub running: bool,
}

/// Reads the state of every application without changing anything.
pub fn status_all() -> Vec<Status> {
    App::ALL.into_iter().map(status).collect()
}

pub fn status(app: App) -> Status {
    let config = app.config_file();
    let recorded = config.as_deref().and_then(|path| recorded_entry(app, path));
    let expected = paths::binary();
    let stale = recorded
        .as_ref()
        .map(|(_, command)| command)
        .filter(|command| !same_program(command, &expected))
        .cloned();

    Status {
        app,
        label: app.label(),
        slug: app.slug(),
        installed: app.installed(),
        config_path: config.as_deref().map(paths::display),
        connected: recorded.is_some(),
        server_key: recorded.map(|(key, _)| key),
        stale_command: stale,
        needs_restart: app.needs_restart(),
        refresh_hint: app.refresh_hint(),
        running: crate::launch::is_running(app),
    }
}

/// Adds Knowlith to an application's server list under the company name.
///
/// Idempotent: connecting an already-connected application rewrites the
/// entry (and migrates a legacy `knowlith` key) and reports success, which
/// is what makes the button in the interface safe to press twice.
pub fn connect(app: App, company: &str) -> Result<Status> {
    let path = app
        .config_file()
        .ok_or(ConnectError::NoConfigLocation(app.label()))?;
    let command = paths::binary();
    let key = server_key(company);

    match app.format() {
        Format::JsonServers => write_json(&path, &command, &key)?,
        Format::TomlServers => write_toml(&path, &command, &key)?,
    }

    // Read back before claiming anything. A write that succeeded and a file
    // that parses are two different facts.
    let confirmed = recorded_entry(app, &path).is_some();
    if !confirmed {
        return Err(ConnectError::Write {
            path: paths::display(&path),
            source: std::io::Error::other("the entry was written but could not be read back"),
        });
    }
    Ok(status(app))
}

/// Rewrites every connected application's key to the current company name.
///
/// Used after a rename so Claude's connectors list does not keep showing
/// yesterday's name next to today's lake.
pub fn rekey_all(company: &str) -> Vec<Status> {
    App::ALL
        .into_iter()
        .filter(|app| status(*app).connected)
        .filter_map(|app| connect(app, company).ok())
        .collect()
}

/// Removes Knowlith from an application's server list, leaving every other
/// server alone. Matches by our binary and by the legacy key, so a rename
/// that left an old company-named entry still disconnects cleanly.
pub fn disconnect(app: App) -> Result<Status> {
    let path = app
        .config_file()
        .ok_or(ConnectError::NoConfigLocation(app.label()))?;
    if !path.exists() {
        return Ok(status(app));
    }

    match app.format() {
        Format::JsonServers => {
            let mut root = read_json(&path)?;
            if let Some(servers) = servers_map_mut(&mut root) {
                remove_our_json(servers);
            }
            backup(&path)?;
            write_atomically(&path, format!("{}\n", serde_json::to_string_pretty(&root).unwrap_or_default()).as_bytes())?;
        }
        Format::TomlServers => {
            let mut document = read_toml(&path)?;
            if let Some(servers) = document
                .get_mut("mcp_servers")
                .and_then(|item| item.as_table_like_mut())
            {
                remove_our_toml(servers);
            }
            backup(&path)?;
            write_atomically(&path, document.to_string().as_bytes())?;
        }
    }
    Ok(status(app))
}

// ------------------------------------------------------------------- json --

fn write_json(path: &Path, command: &str, key: &str) -> Result<()> {
    let mut root = read_json(path)?;

    // Claude Code's `~/.claude.json` carries the owner's entire history and
    // project list. Rebuilding it from a partial model would be a data loss
    // bug with a very long tail, so the document is kept as parsed JSON and
    // exactly one key is set.
    let entry = json!({
        "type": "stdio",
        "command": command,
        "args": ["mcp"],
    });

    let servers = servers_map_mut(&mut root).expect("read_json guarantees an object root");
    // Drop every previous Knowlith entry — legacy product key, yesterday's
    // company name, a stale binary path — before writing today's.
    remove_our_json(servers);
    servers.insert(key.to_string(), entry);

    backup(path)?;
    let text = serde_json::to_string_pretty(&root).map_err(|e| ConnectError::Write {
        path: paths::display(path),
        source: std::io::Error::other(e),
    })?;
    write_atomically(path, format!("{text}\n").as_bytes())
}

fn read_json(path: &Path) -> Result<Value> {
    if !path.exists() {
        return Ok(json!({ "mcpServers": {} }));
    }
    let text = fs::read_to_string(path).map_err(|source| ConnectError::Read {
        path: paths::display(path),
        source,
    })?;
    if text.trim().is_empty() {
        return Ok(json!({ "mcpServers": {} }));
    }

    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(map)) => Ok(Value::Object(map)),
        // A JSON file whose root is an array or a number is not this
        // application's config in any version, so treating it as damaged is
        // the honest reading.
        Ok(_) | Err(_) => {
            let backup = backup(path)?.unwrap_or_default();
            Err(ConnectError::Unreadable {
                path: paths::display(path),
                format: "JSON",
                backup,
            })
        }
    }
}

fn servers_map_mut(root: &mut Value) -> Option<&mut Map<String, Value>> {
    let object = root.as_object_mut()?;
    if !object.contains_key("mcpServers") {
        object.insert("mcpServers".to_string(), json!({}));
    }
    // A config whose `mcpServers` is not an object is replaced rather than
    // merged into: there is nothing there to preserve.
    if !object.get("mcpServers").map(Value::is_object).unwrap_or(false) {
        object.insert("mcpServers".to_string(), json!({}));
    }
    object.get_mut("mcpServers")?.as_object_mut()
}

// ------------------------------------------------------------------- toml --

fn write_toml(path: &Path, command: &str, key: &str) -> Result<()> {
    let mut document = read_toml(path)?;

    let servers = document
        .entry("mcp_servers")
        .or_insert_with(|| toml_edit::Item::Table(implicit_table()));
    let servers = servers
        .as_table_like_mut()
        .ok_or_else(|| ConnectError::Write {
            path: paths::display(path),
            source: std::io::Error::other("mcp_servers is not a table"),
        })?;

    remove_our_toml(servers);

    let mut entry = toml_edit::Table::new();
    entry.insert("command", toml_edit::value(command));
    let mut args = toml_edit::Array::new();
    args.push("mcp");
    entry.insert("args", toml_edit::value(args));
    // Codex gives up on a server that is slow to start. Reading a SQLite
    // file is fast, but a cold disk on a laptop that just woke is not, and a
    // timeout here reads to the owner as "Knowlith is broken".
    entry.insert("startup_timeout_sec", toml_edit::value(20));

    servers.insert(key, toml_edit::Item::Table(entry));

    backup(path)?;
    write_atomically(path, document.to_string().as_bytes())
}

fn implicit_table() -> toml_edit::Table {
    let mut table = toml_edit::Table::new();
    // Writes `[mcp_servers.knowlith]` rather than an empty `[mcp_servers]`
    // header the owner would wonder about.
    table.set_implicit(true);
    table
}

fn read_toml(path: &Path) -> Result<toml_edit::DocumentMut> {
    if !path.exists() {
        return Ok(toml_edit::DocumentMut::new());
    }
    let text = fs::read_to_string(path).map_err(|source| ConnectError::Read {
        path: paths::display(path),
        source,
    })?;
    text.parse::<toml_edit::DocumentMut>().map_err(|_| {
        let backup = backup(path).ok().flatten().unwrap_or_default();
        ConnectError::Unreadable {
            path: paths::display(path),
            format: "TOML",
            backup,
        }
    })
}

// ---------------------------------------------------------------- reading --

/// The command an application's config currently records for Knowlith, with
/// the key it is filed under.
///
/// Prefers an entry whose command is this binary; falls back to the legacy
/// product key so an older install still counts as connected.
fn recorded_entry(app: App, path: &Path) -> Option<(String, String)> {
    let text = fs::read_to_string(path).ok()?;
    let ours = paths::binary();
    match app.format() {
        Format::JsonServers => {
            let root: Value = serde_json::from_str(&text).ok()?;
            let servers = root.get("mcpServers")?.as_object()?;
            let mut legacy = None;
            for (key, value) in servers {
                let Some(command) = value.get("command").and_then(Value::as_str) else {
                    continue;
                };
                if same_program(command, &ours) {
                    return Some((key.clone(), command.to_string()));
                }
                if key == LEGACY_SERVER_NAME {
                    legacy = Some((key.clone(), command.to_string()));
                }
            }
            legacy
        }
        Format::TomlServers => {
            let document = text.parse::<toml_edit::DocumentMut>().ok()?;
            let servers = document.get("mcp_servers")?.as_table_like()?;
            let mut legacy = None;
            for (key, item) in servers.iter() {
                let Some(command) = item.get("command").and_then(|v| v.as_str()) else {
                    continue;
                };
                if same_program(command, &ours) {
                    return Some((key.to_string(), command.to_string()));
                }
                if key == LEGACY_SERVER_NAME {
                    legacy = Some((key.to_string(), command.to_string()));
                }
            }
            legacy
        }
    }
}

fn remove_our_json(servers: &mut Map<String, Value>) {
    let ours = paths::binary();
    let drop: Vec<String> = servers
        .iter()
        .filter_map(|(key, value)| {
            if key == LEGACY_SERVER_NAME {
                return Some(key.clone());
            }
            let command = value.get("command")?.as_str()?;
            same_program(command, &ours).then(|| key.clone())
        })
        .collect();
    for key in drop {
        servers.remove(&key);
    }
}

fn remove_our_toml(servers: &mut dyn toml_edit::TableLike) {
    let ours = paths::binary();
    let drop: Vec<String> = servers
        .iter()
        .filter_map(|(key, item)| {
            if key == LEGACY_SERVER_NAME {
                return Some(key.to_string());
            }
            let command = item.get("command")?.as_str()?;
            same_program(command, &ours).then(|| key.to_string())
        })
        .collect();
    for key in drop {
        servers.remove(&key);
    }
}

/// Whether two recorded commands mean the same program.
///
/// Compared case-insensitively on Windows, where `C:\Users\Ana` and
/// `c:\users\ana` are one path and treating them as two would tell every
/// Windows owner their connection is stale after every restart.
fn same_program(left: &str, right: &str) -> bool {
    if cfg!(windows) {
        left.eq_ignore_ascii_case(right)
    } else {
        left == right
    }
}

// ---------------------------------------------------------------- writing --

/// Copies the current file next to itself before it is changed.
///
/// Returns the backup's path, or `None` when there was nothing to back up.
/// Only one backup per calendar second is kept, which is enough to undo a
/// mistake and not enough to fill a home directory.
fn backup(path: &Path) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let name = format!(
        "{}.knowlith-backup-{stamp}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("config")
    );
    let target = path.with_file_name(name);
    if target.exists() {
        return Ok(Some(paths::display(&target)));
    }
    fs::copy(path, &target).map_err(|source| ConnectError::Write {
        path: paths::display(&target),
        source,
    })?;
    Ok(Some(paths::display(&target)))
}

/// Writes through a temporary file in the same directory, then renames.
///
/// A half-written `claude_desktop_config.json` is not a damaged Knowlith
/// install, it is a Claude Desktop that will not start. The rename is the
/// only operation the filesystem gives us that another process cannot
/// observe halfway through, and the temporary file has to be on the same
/// volume for it to stay atomic — hence the same directory rather than the
/// system temp folder.
fn write_atomically(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| ConnectError::Write {
            path: paths::display(parent),
            source,
        })?;
    }

    let temporary = path.with_extension(format!(
        "knowlith-{}.tmp",
        std::process::id()
    ));

    let write = || -> std::io::Result<()> {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(bytes)?;
        // Without this the rename can land before the contents on a machine
        // that loses power, leaving a correctly named empty file.
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)
    };

    write().map_err(|source| {
        let _ = fs::remove_file(&temporary);
        ConnectError::Write {
            path: paths::display(path),
            source,
        }
    })
}

/// The snippet an owner pastes when they would rather do it by hand.
pub fn manual_instructions(app: App, company: &str) -> String {
    let command = paths::binary();
    let key = server_key(company);
    let path = app
        .config_file()
        .map(|p| paths::display(&p))
        .unwrap_or_else(|| "your configuration file".to_string());

    match app.format() {
        Format::JsonServers => format!(
            "In {path}, inside \"mcpServers\":\n\n  \"{key}\": {{\n    \"type\": \"stdio\",\n    \"command\": \"{}\",\n    \"args\": [\"mcp\"]\n  }}\n",
            command.replace('\\', "\\\\")
        ),
        Format::TomlServers => {
            // Dots and spaces in a company name need a quoted TOML key, or
            // Codex reads nested tables the owner never wrote.
            let keyed = if key.chars().any(|c| !(c.is_ascii_alphanumeric() || c == '_' || c == '-')) {
                format!("\"{key}\"")
            } else {
                key
            };
            format!(
                "In {path}:\n\n[mcp_servers.{keyed}]\ncommand = \"{}\"\nargs = [\"mcp\"]\n",
                command.replace('\\', "\\\\")
            )
        }
    }
}

/// Where backups of a given config file are, newest first.
pub fn backups(app: App) -> Vec<PathBuf> {
    let Some(path) = app.config_file() else {
        return Vec::new();
    };
    let Some(parent) = path.parent() else {
        return Vec::new();
    };
    let prefix = format!(
        "{}.knowlith-backup-",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("")
    );
    let mut found: Vec<PathBuf> = fs::read_dir(parent)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with(&prefix))
                .unwrap_or(false)
        })
        .collect();
    found.sort();
    found.reverse();
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_connectors_list_shows_the_product_and_the_company() {
        assert_eq!(server_key("bb"), "knowlith-bb");
        assert_eq!(server_key("Termoval d.o.o."), "knowlith-termoval-d-o-o");
        assert_eq!(server_key("  "), LEGACY_SERVER_NAME);
        assert_eq!(server_key("Your company"), LEGACY_SERVER_NAME);
    }

    #[test]
    fn a_json_write_files_under_the_company_and_drops_the_legacy_key() {
        let dir = std::env::temp_dir().join(format!(
            "knowlith-connect-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mcp.json");
        let ours = paths::binary();
        fs::write(
            &path,
            format!(
                r#"{{"mcpServers":{{"knowlith":{{"type":"stdio","command":"{ours}","args":["mcp"]}},"other":{{"command":"/bin/true"}}}}}}"#
            ),
        )
        .unwrap();

        write_json(&path, &ours, "knowlith-bb").unwrap();

        let root: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let servers = root["mcpServers"].as_object().unwrap();
        assert!(servers.contains_key("knowlith-bb"));
        assert!(!servers.contains_key("knowlith"));
        assert!(servers.contains_key("other"));
        assert_eq!(
            recorded_entry(App::Cursor, &path).map(|(k, _)| k).as_deref(),
            Some("knowlith-bb")
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn manual_toml_uses_a_plain_slug_key() {
        let text = manual_instructions(App::Codex, "Termoval d.o.o.");
        assert!(text.contains("[mcp_servers.knowlith-termoval-d-o-o]"), "{text}");
        assert!(!text.contains("\"Termoval"), "{text}");
    }
}
