//! The local HTTP API.
//!
//! Bound to `127.0.0.1` and nothing else. This is the whole of what the
//! interface talks to, and it is the reason "your knowledge stays on your
//! machine" survives contact with a web UI: the browser talks to a port on
//! loopback, and the daemon on the other side of it never leaves the disk
//! except through an engine the owner chose.
//!
//! Every endpoint here answers from the lake. Nothing is computed on the fly
//! that a person could not also get from `knowlith` on the command line,
//! which keeps the two views of the same data from drifting apart.

pub mod assets;
pub mod dto;

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::extract::{Path, State};
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use knowlith_core::{ContextObject, Document, ObjectStatus};
use knowlith_lake::Lake;
use serde::Serialize;
use tower_http::cors::{Any, CorsLayer};

use dto::*;

/// The state the interface shows for one AI application.
///
/// Deliberately not a boolean. "Connect" as a single verb hides the four
/// situations an owner actually finds themselves in — the application is
/// not here, it is here and unconnected, it is connected, or it is
/// connected to a Knowlith that no longer exists — and only the last of
/// those is alarming.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolDto {
    slug: String,
    label: String,
    /// `missing` | `ready` | `connected` | `needs-attention`
    state: &'static str,
    installed: bool,
    connected: bool,
    running: bool,
    config_path: Option<String>,
    needs_restart: bool,
    refresh_hint: &'static str,
    /// Set when the entry points at another binary — after an upgrade that
    /// moved it, or a copy someone deleted.
    problem: Option<String>,
    /// When this application last read something, so the page can say
    /// "used 3 minutes ago" rather than only "connected".
    last_read: Option<String>,
    reads: i64,
}

/// The lake behind a mutex.
///
/// SQLite in WAL mode handles concurrent readers, but `approve` takes a write
/// transaction and the whole daemon is one process serving one person. A
/// mutex is the honest amount of machinery for that; a connection pool would
/// be ceremony around a problem this does not have.
#[derive(Clone)]
pub struct AppState {
    lake: Arc<Mutex<Lake>>,
    company: Arc<str>,
}

impl AppState {
    pub fn new(lake: Lake, company: impl Into<Arc<str>>) -> Self {
        Self {
            lake: Arc::new(Mutex::new(lake)),
            company: company.into(),
        }
    }
}

type ApiResult<T> = std::result::Result<Json<T>, (StatusCode, String)>;

fn failed(e: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

pub fn router(state: AppState) -> Router {
    // The interface is served by Vite during development, on another port.
    // Loopback-only binding is what keeps this safe, not the origin header.
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/api/health", get(health))
        .route("/api/company", get(company).put(rename_company))
        .route("/api/objects", get(objects))
        .route("/api/review", get(review))
        .route("/api/review/{id}/approve", post(approve))
        .route("/api/review/{id}/reject", post(reject))
        .route("/api/sources", get(sources).post(add_source))
        .route("/api/sources/browse", post(browse))
        .route("/api/sources/preview", get(preview_source))
        .route("/api/skills", get(skills))
        .route("/api/merge-hints", get(merge_hints))
        .route("/api/merge-hints/{keep}/{drop}/merge", post(merge))
        .route("/api/merge-hints/{keep}/{drop}/dismiss", post(dismiss))
        .route("/api/discovery", get(discovery))
        .route("/api/runs", get(runs))
        .route("/api/documents", get(documents))
        .route("/api/tool-reads", get(tool_reads))
        .route("/api/activity", get(activity))
        .route("/api/usage", get(usage))
        .route("/api/tree", get(tree))
        .route("/api/tools", get(tools))
        .route("/api/tools/{app}/connect", post(connect_app))
        .route("/api/tools/{app}/disconnect", post(disconnect_app))
        .route("/api/tools/{app}/open", post(open_app))
        .route("/api/tools/{app}/preview", get(preview_app))
        .route("/api/bundle", post(build_bundle))
        .route("/api/policy", get(read_policy).put(write_policy))
        .route("/api/work/release", post(release_work))
        .route("/api/autostart", get(read_autostart))
        .route("/api/autostart/{state}", post(write_autostart))
        .layer(cors)
        .with_state(state)
        // Anything that is not an API route is the interface. Registered
        // last so a future endpoint can never be shadowed by a file.
        .fallback(interface)
}

pub async fn serve(state: AppState, port: u16) -> anyhow::Result<()> {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(address).await?;
    println!("Knowlith is listening on http://{address}");
    axum::serve(listener, router(state)).await?;
    Ok(())
}

// ------------------------------------------------------------- endpoints --

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Health {
    ok: bool,
    documents: i64,
    objects: usize,
    /// What the background worker still has to do. The interface tells the
    /// owner "you can close this window" — this is what makes that sentence
    /// checkable rather than reassuring.
    queued: i64,
    working: i64,
}

async fn health(State(state): State<AppState>) -> ApiResult<Health> {
    let lake = state.lake.lock().map_err(failed)?;
    let counts = lake.job_counts().map_err(failed)?;
    let count_of = |state: &str| counts.iter().find(|(s, _)| s == state).map(|(_, n)| *n).unwrap_or(0);

    Ok(Json(Health {
        ok: true,
        documents: lake.document_count().map_err(failed)?,
        objects: lake.object_ids(None).map_err(failed)?.len(),
        queued: count_of("queued"),
        working: count_of("leased"),
    }))
}

#[derive(Serialize)]
struct Company {
    name: String,
}

async fn company(State(state): State<AppState>) -> ApiResult<Company> {
    let lake = state.lake.lock().map_err(failed)?;
    Ok(Json(Company { name: lake.company() }))
}

#[derive(serde::Deserialize)]
struct Rename {
    name: String,
}

/// Names the company, from onboarding or from settings.
///
/// Stored in the lake, so the gateway and the extension say the same thing
/// without the daemon being restarted.
async fn rename_company(
    State(state): State<AppState>,
    Json(rename): Json<Rename>,
) -> ApiResult<Company> {
    if rename.name.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "a company needs a name".into()));
    }
    let lake = state.lake.lock().map_err(failed)?;
    lake.set_company(&rename.name).map_err(failed)?;
    Ok(Json(Company { name: lake.company() }))
}

async fn objects(State(state): State<AppState>) -> ApiResult<Vec<ObjectDto>> {
    let lake = state.lake.lock().map_err(failed)?;
    let documents = lake.documents().map_err(failed)?;
    Ok(Json(
        lake.objects()
            .map_err(failed)?
            .iter()
            // The tree shows what the company has; rejected claims are kept
            // so they are not re-proposed, but they are not knowledge.
            .filter(|o| o.status != ObjectStatus::Rejected)
            .map(|o| object_dto(o, &documents))
            .collect(),
    ))
}

/// Everything waiting for a person.
///
/// A review item is not a separate record: it is an object that has not been
/// decided yet, plus the diff against whatever it replaces. Storing it twice
/// would be two things to keep in agreement.
async fn review(State(state): State<AppState>) -> ApiResult<Vec<ReviewItemDto>> {
    let lake = state.lake.lock().map_err(failed)?;
    let documents = lake.documents().map_err(failed)?;
    let all = lake.objects().map_err(failed)?;

    let items = all
        .iter()
        .filter(|o| matches!(o.status, ObjectStatus::Proposed | ObjectStatus::Conflicted))
        .map(|object| review_item(object, &all, &documents))
        .collect();

    Ok(Json(items))
}

fn review_item(object: &ContextObject, all: &[ContextObject], documents: &[Document]) -> ReviewItemDto {
    let find_doc = |id: &str| documents.iter().find(|d| d.id == id);

    // What this would change: everything that names it.
    let affects: Vec<RelationDto> = all
        .iter()
        .filter_map(|other| {
            let edge = other.relations.iter().find(|r| r.target_id == object.id)?;
            Some(RelationDto {
                kind: "used_by",
                target_id: other.id.clone(),
                target_title: other.title.clone(),
                // Carried through rather than assumed: an owner deciding
                // what a change affects should be able to see which of
                // those consequences are a machine's suggestion.
                origin: origin_str(edge.origin),
            })
        })
        .collect();

    // A superseded version, when there is one, is the "before" side of the
    // diff — the owner is deciding between two real texts, not between a
    // text and a blank.
    let before = object
        .supersedes
        .as_ref()
        .and_then(|id| all.iter().find(|o| o.id == *id || o.path == *id))
        .map(|o| o.body.clone());

    let conflict = (object.status == ObjectStatus::Conflicted).then(|| {
        let sides = object
            .evidence
            .iter()
            .take(2)
            .map(|e| ConflictSideDto {
                label: find_doc(&e.document_id)
                    .map(|d| d.name.clone())
                    .unwrap_or_else(|| e.document_id.clone()),
                value: figure(&e.quote),
                evidence: evidence_dto(e, find_doc(&e.document_id)),
            })
            .collect();
        ConflictDto {
            summary: format!(
                "Two documents state something different about {}.",
                object.title.to_lowercase()
            ),
            sides,
        }
    });

    // Two documents covering one subject, with no figures to disagree about.
    //
    // The compiler folds them into one object and keeps every quote, which
    // is correct — but it meant the older document's sentence was stored and
    // never shown, and the owner saw one wording with no sign that a second
    // document also spoke. Saying "these both cover it, here is each" claims
    // nothing that cannot be checked; saying "these disagree" would claim
    // something that cannot.
    let coverage = (object.status != ObjectStatus::Conflicted)
        .then(|| {
            let mut seen: Vec<&str> = Vec::new();
            let mut sources: Vec<CoverageSourceDto> = Vec::new();
            for span in &object.evidence {
                if seen.contains(&span.document_id.as_str()) {
                    continue;
                }
                seen.push(&span.document_id);
                let document = find_doc(&span.document_id);
                sources.push(CoverageSourceDto {
                    label: document
                        .map(|d| d.name.clone())
                        .unwrap_or_else(|| span.document_id.clone()),
                    modified: document.map(|d| d.modified.clone()).unwrap_or_default(),
                    current: false,
                    evidence: evidence_dto(span, document),
                });
            }
            if sources.len() < 2 {
                return None;
            }
            // The wording in use came from the most recent document. That is
            // the same rule stage 3 applied when it chose it.
            if let Some(newest) = sources
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.modified.cmp(&b.1.modified))
                .map(|(index, _)| index)
            {
                sources[newest].current = true;
            }
            sources.sort_by(|a, b| b.modified.cmp(&a.modified));
            Some(CoverageDto {
                summary: format!(
                    "{} documents cover this. The most recent wording is the one in use.",
                    sources.len()
                ),
                sources,
            })
        })
        .flatten();

    ReviewItemDto {
        id: format!("rev-{}", object.id),
        object_id: object.id.clone(),
        kind: kind_str(object.kind),
        title: object.title.clone(),
        before,
        after: object.body.clone(),
        evidence: object
            .evidence
            .iter()
            .map(|e| evidence_dto(e, find_doc(&e.document_id)))
            .collect(),
        confidence: object.confidence.0,
        affects,
        conflict,
        coverage,
        compiled_at: object.updated_at.clone(),
    }
}

/// The first figure in a sentence, for the side-by-side conflict panel.
///
/// "8%" against "5%" is a decision an owner can make at a glance; two
/// paragraphs of prose is not.
fn figure(quote: &str) -> String {
    let chars: Vec<char> = quote.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.' || chars[i] == ',') {
                i += 1;
            }
            let mut value: String = chars[start..i].iter().collect();
            let rest: String = chars[i..].iter().collect();
            for unit in ["%", " dana", " mjeseci", " sati", " EUR"] {
                if rest.starts_with(unit) {
                    value.push_str(unit);
                    break;
                }
            }
            return value;
        }
        i += 1;
    }
    String::new()
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Decision {
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    edited: bool,
    #[serde(default)]
    decided_by: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Approved {
    /// What now rests on something that moved, and should be looked at.
    affected: Vec<String>,
}

async fn approve(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Option<Json<Decision>>,
) -> ApiResult<Approved> {
    let object_id = id.strip_prefix("rev-").unwrap_or(&id).to_string();
    let decision = body.map(|Json(d)| d).unwrap_or_default();

    let mut lake = state.lake.lock().map_err(failed)?;
    let current = lake
        .objects()
        .map_err(failed)?
        .into_iter()
        .find(|o| o.id == object_id)
        .ok_or((StatusCode::NOT_FOUND, format!("no object {object_id}")))?;

    let text = decision.body.unwrap_or(current.body);
    let was_process = current.kind == knowlith_core::ObjectKind::Process;
    let affected = lake
        .approve(
            &object_id,
            &text,
            decision.decided_by.as_deref().unwrap_or("You"),
            decision.edited,
        )
        .map_err(failed)?;

    // Approving a process is the event that makes a skill possible, so the
    // request is queued here rather than waiting for a schedule. If no
    // worker is running the job simply sits there, which is the honest
    // outcome: the work is recorded and will happen when something drains
    // the queue.
    if was_process {
        let _ = knowlith_worker::enqueue_skill_drafting(&lake);
    }

    Ok(Json(Approved { affected }))
}

async fn reject(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult<Approved> {
    let object_id = id.strip_prefix("rev-").unwrap_or(&id);
    let lake = state.lake.lock().map_err(failed)?;
    lake.reject(object_id).map_err(failed)?;
    Ok(Json(Approved { affected: Vec::new() }))
}

/// Opens the machine's own folder chooser and reports what was picked.
///
/// The browser cannot answer this question — it is never told where a folder
/// is — so the daemon answers it instead, with the same dialog every other
/// application on this machine uses. `chosen: null` means the owner closed
/// it, which is an ordinary thing to do and not an error.
///
/// The walk is done here too: by the time the dialog closes, the owner is
/// already looking at the folder they picked and wants to know what is in it.
async fn browse(State(state): State<AppState>) -> ApiResult<BrowseDto> {
    let company = state.company.clone();
    // The dialog blocks until somebody clicks, which would otherwise hold an
    // async worker thread for as long as the chooser is open.
    let picked = tokio::task::spawn_blocking(move || {
        knowlith_desktop::pick::folder(&format!("Choose a folder for {company}"))
    })
    .await
    .map_err(failed)?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let Some(path) = picked else {
        return Ok(Json(BrowseDto { chosen: None, name: None, inventory: None }));
    };

    let found = tokio::task::spawn_blocking({
        let path = path.clone();
        move || knowlith_extract::inventory::inventory(&path)
    })
    .await
    .map_err(failed)?;

    Ok(Json(BrowseDto {
        name: Some(folder_name(&path)),
        chosen: Some(knowlith_desktop::paths::display(&path)),
        inventory: Some(found),
    }))
}

/// Counts what is in a folder without reading any of it.
///
/// Used for a path the owner typed, and again when the interface comes back
/// to a folder it already knows about.
async fn preview_source(
    State(_state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<PathQuery>,
) -> ApiResult<BrowseDto> {
    let path = resolve(&query.path)?;
    let found = tokio::task::spawn_blocking({
        let path = path.clone();
        move || knowlith_extract::inventory::inventory(&path)
    })
    .await
    .map_err(failed)?;

    Ok(Json(BrowseDto {
        name: Some(folder_name(&path)),
        chosen: Some(knowlith_desktop::paths::display(&path)),
        inventory: Some(found),
    }))
}

/// Adds a folder and asks for it to be walked.
///
/// Nothing is scanned on this thread. The source is recorded and a job is
/// queued, and the worker that is already running does the walking — so the
/// reading carries on after the window is closed, which is the whole claim
/// the background daemon makes.
async fn add_source(
    State(state): State<AppState>,
    Json(body): Json<NewSource>,
) -> ApiResult<SourceAddedDto> {
    let path = resolve(&body.path)?;
    let root = knowlith_desktop::paths::display(&path);
    let name = body
        .name
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| folder_name(&path));

    let lake = state.lake.lock().map_err(failed)?;

    // A folder already being watched is told so rather than added twice
    // under a second id, which would double every document in the lake.
    for (id, existing_name, existing_root, _, _, _) in lake.sources().map_err(failed)? {
        let same = knowlith_desktop::paths::real(std::path::Path::new(&existing_root))
            .is_ok_and(|real| real == path);
        if same {
            return Ok(Json(SourceAddedDto {
                id,
                name: existing_name,
                path: root,
                queued: false,
                already_known: true,
            }));
        }
    }

    let id = source_id(&name, &root);
    lake.put_source(&id, &name, &root, "folder", "codex")
        .map_err(failed)?;
    let queued = knowlith_worker::enqueue_rescan(&lake, &id, &root).map_err(failed)?;

    Ok(Json(SourceAddedDto {
        id,
        name,
        path: root,
        queued,
        already_known: false,
    }))
}

/// A path the owner gave us, turned into one we can walk — or a reason why not.
///
/// Every failure here is the owner's to fix, so each one says which of the
/// three things went wrong rather than "invalid path".
fn resolve(given: &str) -> std::result::Result<std::path::PathBuf, (StatusCode, String)> {
    let trimmed = given.trim();
    if trimmed.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "no folder was given".into()));
    }
    // `~/Documents/…` is what somebody types, and it is not a path until the
    // home directory is put back in front of it.
    let expanded = match trimmed.strip_prefix("~/") {
        Some(rest) => knowlith_desktop::paths::home().join(rest),
        None => std::path::PathBuf::from(trimmed),
    };
    let real = knowlith_desktop::paths::real(&expanded).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            format!("there is nothing at {}", expanded.display()),
        )
    })?;
    if !real.is_dir() {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("{} is a file, not a folder", real.display()),
        ));
    }
    // Readable is checked now rather than discovered by a job that fails
    // twenty minutes later in a log nobody opens.
    std::fs::read_dir(&real).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("{} cannot be read: {e}", real.display()),
        )
    })?;
    Ok(real)
}

fn folder_name(path: &std::path::Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("folder")
        .to_string()
}

/// A stable id for a folder.
///
/// Derived from the path so the same folder added again is recognised as the
/// same source even if the lake was rebuilt in between.
fn source_id(name: &str, root: &str) -> String {
    let slug: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    // " - " becomes three dashes, so one pass of replace leaves two.
    let mut slug = slug.trim_matches('-').to_string();
    while slug.contains("--") {
        slug = slug.replace("--", "-");
    }
    let digest = knowlith_core::sha256_hex(root.as_bytes());
    format!("{}-{}", if slug.is_empty() { "folder" } else { &slug }, &digest[..8])
}

async fn sources(State(state): State<AppState>) -> ApiResult<Vec<SourceDto>> {
    let lake = state.lake.lock().map_err(failed)?;
    let documents = lake.documents().map_err(failed)?;
    let objects = lake.objects().map_err(failed)?;

    let waiting = objects
        .iter()
        .filter(|o| o.status == ObjectStatus::Proposed)
        .count();
    let conflicts = objects
        .iter()
        .filter(|o| o.status == ObjectStatus::Conflicted)
        .count();

    let mut out = Vec::new();
    for (id, name, root, kind, status, last_scan) in lake.sources().map_err(failed)? {
        let mine: Vec<&Document> = documents.iter().collect();
        let mut types: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
        for document in &mine {
            let ext = document
                .name
                .rsplit_once('.')
                .map(|(_, e)| e.to_uppercase())
                .unwrap_or_default();
            *types.entry(ext).or_default() += 1;
        }
        let mut file_types: Vec<FileTypeDto> = types
            .into_iter()
            .map(|(ext, count)| FileTypeDto { ext, count })
            .collect();
        file_types.sort_by_key(|entry| std::cmp::Reverse(entry.count));

        let scanned = last_scan.unwrap_or_default();
        out.push(SourceDto {
            id,
            name,
            path: root,
            kind: if kind == "nas" { "nas" } else { "folder" },
            // Not a setting. Knowlith has no code path that writes into a
            // source, so this can only ever be one value.
            access: "read-only",
            file_count: mine.len(),
            bytes: mine.iter().map(|d| d.byte_len).sum(),
            last_sync: scanned.clone(),
            processor: "codex",
            status,
            last_analyzed: scanned,
            changes_found: waiting,
            conflicts_found: conflicts,
            file_types,
        });
    }
    Ok(Json(out))
}

/// The skills drafted from approved knowledge.
///
/// A skill is a `ContextObject` like any other, so it arrives here with its
/// inherited spans and its edges already attached. Nothing is assembled on
/// this path that `knowlith show` could not also print.
async fn skills(State(state): State<AppState>) -> ApiResult<Vec<SkillDocDto>> {
    let lake = state.lake.lock().map_err(failed)?;
    let documents = lake.documents().map_err(failed)?;
    let all = lake.objects().map_err(failed)?;

    let out = all
        .iter()
        .filter(|o| o.kind == knowlith_core::ObjectKind::Skill && o.status != ObjectStatus::Rejected)
        .map(|skill| SkillDocDto {
            id: skill.id.clone(),
            name: skill.title.clone(),
            description: skill_description(&skill.body),
            markdown: skill.body.clone(),
            requires: skill
                .relations
                .iter()
                .filter(|r| r.kind == knowlith_core::RelationType::DependsOn)
                .map(relation_dto)
                .collect(),
            inputs: skill_fields(&skill.body, "**Before you start**"),
            outputs: skill_fields(&skill.body, "**Result**"),
            affects: all
                .iter()
                .filter(|other| other.relations.iter().any(|r| r.target_id == skill.id))
                .filter_map(|other| {
                    let edge = other.relations.iter().find(|r| r.target_id == skill.id)?;
                    Some(RelationDto {
                        kind: "used_by",
                        target_id: other.id.clone(),
                        target_title: other.title.clone(),
                        origin: origin_str(edge.origin),
                    })
                })
                .collect(),
            evidence: skill
                .evidence
                .iter()
                .map(|e| evidence_dto(e, documents.iter().find(|d| d.id == e.document_id)))
                .collect(),
            status: status_str(skill.status),
            version: skill.version,
            updated_at: skill.updated_at.clone(),
        })
        .collect();

    Ok(Json(out))
}

// -------------------------------------------------------- merge hints ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MergeHintDto {
    keep_id: String,
    keep_title: String,
    keep_body: String,
    drop_id: String,
    drop_title: String,
    drop_body: String,
    /// `duplicate` — one decision written twice, merging loses nothing.
    /// `disagreement` — one subject, two live figures, and an agent asked
    /// today would answer with whichever it saw first.
    kind: String,
    score: f32,
}

/// Pairs that may be one rule written twice.
///
/// Nothing on this path merges anything. The daemon found two objects whose
/// subjects overlap and cannot tell whether they are the same decision; the
/// owner can, in about two seconds, and their answer is kept.
async fn merge_hints(State(state): State<AppState>) -> ApiResult<Vec<MergeHintDto>> {
    let lake = state.lake.lock().map_err(failed)?;
    Ok(Json(
        lake.open_merge_hints()
            .map_err(failed)?
            .into_iter()
            .map(|hint| MergeHintDto {
                keep_id: hint.left_id,
                keep_title: hint.left_title,
                keep_body: hint.left_body,
                drop_id: hint.right_id,
                drop_title: hint.right_title,
                drop_body: hint.right_body,
                kind: hint.kind,
                score: hint.score,
            })
            .collect(),
    ))
}

async fn merge(
    State(state): State<AppState>,
    Path((keep, drop)): Path<(String, String)>,
) -> ApiResult<Approved> {
    let mut lake = state.lake.lock().map_err(failed)?;
    lake.merge_objects(&keep, &drop).map_err(failed)?;
    Ok(Json(Approved { affected: vec![drop] }))
}

async fn dismiss(
    State(state): State<AppState>,
    Path((keep, drop)): Path<(String, String)>,
) -> ApiResult<Approved> {
    let lake = state.lake.lock().map_err(failed)?;
    lake.dismiss_merge_hint(&keep, &drop).map_err(failed)?;
    Ok(Json(Approved { affected: Vec::new() }))
}

async fn tree() -> ApiResult<Vec<serde_json::Value>> {
    Ok(Json(Vec::new()))
}

async fn runs() -> ApiResult<Vec<serde_json::Value>> {
    Ok(Json(Vec::new()))
}

async fn discovery(State(state): State<AppState>) -> ApiResult<DiscoveryDto> {
    let lake = state.lake.lock().map_err(failed)?;
    let objects = lake.objects().map_err(failed)?;
    let count = |kind| objects.iter().filter(|o| o.kind == kind).count();

    Ok(Json(DiscoveryDto {
        rules: count(knowlith_core::ObjectKind::Rule),
        processes: count(knowlith_core::ObjectKind::Process),
        terms: count(knowlith_core::ObjectKind::Term) + count(knowlith_core::ObjectKind::Fact),
        skills: count(knowlith_core::ObjectKind::Skill),
        conflicts: objects
            .iter()
            .filter(|o| o.status == ObjectStatus::Conflicted)
            .count(),
        files_read: lake.document_count().map_err(failed)? as usize,
        spans_extracted: lake.block_count().map_err(failed)? as usize,
        duration_seconds: 0,
    }))
}

async fn documents(State(state): State<AppState>) -> ApiResult<Vec<SourceDocumentDto>> {
    let lake = state.lake.lock().map_err(failed)?;
    Ok(Json(
        lake.documents().map_err(failed)?.iter().map(document_dto).collect(),
    ))
}

async fn tool_reads(
    State(state): State<AppState>,
) -> ApiResult<std::collections::HashMap<String, Vec<ToolReadDto>>> {
    let lake = state.lake.lock().map_err(failed)?;
    let mut out = std::collections::HashMap::new();
    for id in lake.object_ids(None).map_err(failed)? {
        let reads = lake.reads_of(&id).map_err(failed)?;
        if !reads.is_empty() {
            out.insert(
                id,
                reads
                    .into_iter()
                    .map(|(tool, last_read_at)| ToolReadDto { tool, last_read_at })
                    .collect(),
            );
        }
    }
    Ok(Json(out))
}

/// What happened recently, built from what is recorded rather than logged
/// separately.
async fn activity(State(state): State<AppState>) -> ApiResult<Vec<ActivityDto>> {
    let lake = state.lake.lock().map_err(failed)?;
    let documents = lake.documents().map_err(failed)?;
    let mut objects = lake.objects().map_err(failed)?;
    objects.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    let out = objects
        .iter()
        .take(6)
        .map(|object| ActivityDto {
            id: format!("act-{}", object.id),
            title: match object.status {
                ObjectStatus::Approved => format!("{} approved", object.title),
                ObjectStatus::Conflicted => format!("{} — documents disagree", object.title),
                _ => format!("{} found", object.title),
            },
            // The file's name, never its id. "From doc:9982ba86ea32837c"
            // describes the owner's own document in a vocabulary they have
            // no way to read.
            detail: object
                .evidence
                .first()
                .map(|e| {
                    let name = documents
                        .iter()
                        .find(|d| d.id == e.document_id)
                        .map(|d| d.name.as_str())
                        .unwrap_or("a document that is no longer there");
                    format!("From {}, {}.", name, e.locator)
                })
                .unwrap_or_default(),
            source: "default".into(),
            at: object.updated_at.clone(),
            tone: match object.status {
                ObjectStatus::Conflicted => "conflict",
                ObjectStatus::Proposed => "pending",
                _ => "neutral",
            },
        })
        .collect();

    Ok(Json(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_figure_is_what_the_conflict_panel_shows() {
        assert_eq!(figure("Komercijalist odobrava popust do 8% na cijenu."), "8%");
        assert_eq!(figure("Rok plaćanja je 15 dana od izdavanja."), "15 dana");
        assert_eq!(figure("Nema brojeva ovdje."), "");
    }

    #[test]
    fn the_interfaces_words_are_used_on_the_wire() {
        assert_eq!(status_str(ObjectStatus::Proposed), "draft");
        assert_eq!(status_str(ObjectStatus::Conflicted), "conflict");
    }
}


// -------------------------------------------------------- AI applications --

/// Every application, what it can see, and whether it has used it.
async fn tools(State(state): State<AppState>) -> ApiResult<Vec<ToolDto>> {
    let lake = state.lake.lock().map_err(failed)?;
    // Per application now, from `clientInfo`. A read taken before the
    // gateway recorded who was asking has no application and is counted
    // against none of them rather than divided between them.
    let used = lake.use_by_app().unwrap_or_default();

    let out = knowlith_desktop::status_all()
        .into_iter()
        .map(|status| {
            let mine = used.iter().find(|u| u.app.as_deref() == Some(status.slug));
            ToolDto {
                state: match (&status.installed, &status.connected, &status.stale_command) {
                    (false, ..) => "missing",
                    (true, _, Some(_)) => "needs-attention",
                    (true, true, None) => "connected",
                    (true, false, None) => "ready",
                },
                slug: status.slug.to_string(),
                label: status.label.to_string(),
                installed: status.installed,
                connected: status.connected,
                running: status.running,
                config_path: status.config_path,
                needs_restart: status.needs_restart,
                refresh_hint: status.refresh_hint,
                problem: status.stale_command.map(|command| {
                    format!("It is pointing at {command}, which is not this Knowlith.")
                }),
                last_read: mine.and_then(|u| u.last_read_at.clone()),
                reads: mine.map(|u| u.objects).unwrap_or(0),
            }
        })
        .collect();
    Ok(Json(out))
}

/// What the AI tools on this machine have actually done, newest first.
///
/// One row per question, not one per protocol call. An agent asking about a
/// quotation makes five or six calls; the owner wants to see one thing that
/// happened, with what it read underneath.
///
/// Reads that belong to no case are folded into rows of their own. Most
/// agents never open a case, and leaving those out would make this screen
/// claim nothing is happening while the gateway is busy.
async fn usage(State(state): State<AppState>) -> ApiResult<Vec<UsageDto>> {
    const LIMIT: usize = 40;

    let lake = state.lake.lock().map_err(failed)?;
    let objects = lake.objects().map_err(failed)?;
    let named = |id: &str| -> Option<UsedObjectDto> {
        objects.iter().find(|o| o.id == id).map(|o| UsedObjectDto {
            id: o.id.clone(),
            title: o.title.clone(),
            kind: kind_str(o.kind).to_string(),
        })
    };

    let mut out = Vec::new();

    for case in lake.recent_cases(LIMIT).map_err(failed)? {
        let read_ids = lake.case_reads(&case.id).unwrap_or_default();
        let read: Vec<UsedObjectDto> = read_ids.iter().filter_map(|id| named(id)).collect();
        // Named as relevant when the case opened, never opened since. The
        // comparison is against what the agent was told then, not against
        // the lake now, or approving something mid-conversation would make
        // a finished case look incomplete.
        let skipped: Vec<UsedObjectDto> = case
            .areas
            .iter()
            .filter(|id| !read_ids.contains(id))
            .filter_map(|id| named(id))
            .collect();

        out.push(UsageDto {
            id: case.id.clone(),
            app_label: app_label(case.app.as_deref()),
            app: case.app,
            question: Some(case.question),
            at: case.opened_at,
            read,
            skipped,
            closed: case.closed_at.is_some(),
        });
    }

    for (object_id, tool, app, at) in lake.loose_reads(LIMIT).map_err(failed)? {
        let Some(object) = named(&object_id) else {
            // `lookup_value` records against "table" rather than an object,
            // because a row in a spreadsheet is not a context object. It is
            // a real read and it belongs on the trail, named for what it is.
            if object_id == "table" {
                out.push(UsageDto {
                    id: format!("read:{tool}:{at}"),
                    app_label: app_label(app.as_deref()),
                    app,
                    question: None,
                    at,
                    read: vec![UsedObjectDto {
                        id: "table".into(),
                        title: "A figure, read from the row it is written in".into(),
                        kind: "fact".into(),
                    }],
                    skipped: Vec::new(),
                    closed: true,
                });
            }
            continue;
        };
        out.push(UsageDto {
            id: format!("read:{object_id}:{at}"),
            app_label: app_label(app.as_deref()),
            app,
            question: None,
            at,
            read: vec![object],
            skipped: Vec::new(),
            closed: true,
        });
    }

    out.sort_by(|a, b| b.at.cmp(&a.at));
    out.truncate(LIMIT);
    Ok(Json(out))
}

/// What to call the application on screen.
///
/// An unrecognised client is "another tool" and never one of the three we
/// know: telling an owner that Codex read their rules when it was something
/// else is worse than telling them nothing.
fn app_label(slug: Option<&str>) -> String {
    slug.and_then(knowlith_desktop::App::parse)
        .map(|app| app.label().to_string())
        .unwrap_or_else(|| "Another tool".to_string())
}

fn app_named(slug: &str) -> std::result::Result<knowlith_desktop::App, (StatusCode, String)> {
    knowlith_desktop::App::parse(slug)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("no application called {slug}")))
}

/// What connecting would write, so the owner sees it before agreeing.
async fn preview_app(Path(app): Path<String>) -> ApiResult<serde_json::Value> {
    let app = app_named(&app)?;
    Ok(Json(serde_json::json!({
        "configPath": app.config_file().as_deref().map(knowlith_desktop::paths::display),
        "snippet": knowlith_desktop::connect::manual_instructions(app),
        "refreshHint": app.refresh_hint(),
    })))
}

async fn connect_app(
    State(state): State<AppState>,
    Path(app): Path<String>,
) -> ApiResult<serde_json::Value> {
    let app = app_named(&app)?;
    let status = knowlith_desktop::connect(app).map_err(|e| (StatusCode::CONFLICT, e.to_string()))?;

    // The standing instruction is what makes the agent reach for the company
    // unprompted. Failing to write it does not undo the connection, so it is
    // reported rather than thrown.
    let guide = match app {
        knowlith_desktop::App::Codex => Some(knowlith_desktop::Guide::Codex),
        knowlith_desktop::App::ClaudeCode => Some(knowlith_desktop::Guide::ClaudeCode),
        knowlith_desktop::App::ClaudeDesktop => None,
    };
    let guidance = guide
        .map(|guide| knowlith_desktop::guidance::write(guide, &state.company).is_ok())
        .unwrap_or(false);

    Ok(Json(serde_json::json!({
        "connected": status.connected,
        "configPath": status.config_path,
        "refreshHint": status.refresh_hint,
        "needsRestart": status.needs_restart,
        "running": status.running,
        "guidanceWritten": guidance,
    })))
}

async fn disconnect_app(Path(app): Path<String>) -> ApiResult<serde_json::Value> {
    let app = app_named(&app)?;
    let status = knowlith_desktop::disconnect(app).map_err(failed)?;
    if let Some(guide) = match app {
        knowlith_desktop::App::Codex => Some(knowlith_desktop::Guide::Codex),
        knowlith_desktop::App::ClaudeCode => Some(knowlith_desktop::Guide::ClaudeCode),
        knowlith_desktop::App::ClaudeDesktop => None,
    } {
        let _ = knowlith_desktop::guidance::remove(guide);
    }
    Ok(Json(serde_json::json!({ "connected": status.connected })))
}

async fn open_app(Path(app): Path<String>) -> ApiResult<serde_json::Value> {
    let app = app_named(&app)?;
    let outcome = knowlith_desktop::open_or_restart(app);
    Ok(Json(serde_json::json!({
        "outcome": outcome,
        "message": outcome.message(app),
    })))
}

/// Builds the Claude Desktop extension and, by default, opens it.
async fn build_bundle(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let icon = serde_json::to_value(knowlith_desktop::company_icon(&state.company)).map_err(failed)?;
    let tools: Vec<(String, String)> = knowlith_mcp::tools::catalogue(&icon)
        .into_iter()
        .map(|entry| (text_of(&entry, "name"), text_of(&entry, "description")))
        .collect();
    let prompts: Vec<(String, String)> = {
        let lake = state.lake.lock().map_err(failed)?;
        knowlith_mcp::prompts::list(&lake, &icon)
            .into_iter()
            .map(|entry| (text_of(&entry, "name"), text_of(&entry, "description")))
            .collect()
    };

    let built = knowlith_desktop::bundle::build(&knowlith_desktop::bundle::Contents {
        company: &state.company,
        tools,
        prompts,
    })
    .map_err(failed)?;
    knowlith_desktop::bundle::reveal(&built);

    Ok(Json(serde_json::json!({
        "path": built.path,
        "megabytes": (built.bytes as f64 / 1_048_576.0 * 10.0).round() / 10.0,
        "version": built.version,
    })))
}

fn text_of(value: &serde_json::Value, key: &str) -> String {
    value.get(key).and_then(|v| v.as_str()).unwrap_or_default().to_string()
}

// -------------------------------------------------- background processing --

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PolicyDto {
    policy: knowlith_lake::Policy,
    /// What is sitting still and why, so the interface offers the button
    /// that actually helps rather than a generic "retry".
    held: Vec<HeldDto>,
    on_battery: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HeldDto {
    kind: String,
    reason: String,
    count: i64,
}

async fn read_policy(State(state): State<AppState>) -> ApiResult<PolicyDto> {
    let lake = state.lake.lock().map_err(failed)?;
    Ok(Json(PolicyDto {
        policy: lake.policy(),
        held: lake
            .held()
            .unwrap_or_default()
            .into_iter()
            .map(|(kind, reason, count)| HeldDto { kind, reason, count })
            .collect(),
        on_battery: knowlith_desktop::power().on_battery(),
    }))
}

async fn write_policy(
    State(state): State<AppState>,
    Json(policy): Json<knowlith_lake::Policy>,
) -> ApiResult<PolicyDto> {
    {
        let lake = state.lake.lock().map_err(failed)?;
        lake.set_policy(&policy).map_err(failed)?;
        // Turning automatic reading back on should start the work that was
        // waiting for exactly that, without the owner pressing a second
        // button they have no reason to know about.
        if policy.processing == knowlith_lake::Processing::Automatic {
            let _ = lake.release_held();
        }
    }
    read_policy(State(state)).await
}

/// Lets everything that was held run now.
async fn release_work(State(state): State<AppState>) -> ApiResult<serde_json::Value> {
    let lake = state.lake.lock().map_err(failed)?;
    let released = lake.release_held().map_err(failed)?;
    Ok(Json(serde_json::json!({ "released": released })))
}

// ------------------------------------------------------ the login service --

async fn read_autostart() -> ApiResult<knowlith_desktop::Autostart> {
    Ok(Json(knowlith_desktop::autostart::status()))
}

async fn write_autostart(Path(state): Path<String>) -> ApiResult<knowlith_desktop::Autostart> {
    let result = match state.as_str() {
        "on" => knowlith_desktop::autostart::enable(7717),
        "off" => knowlith_desktop::autostart::disable(),
        other => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("{other} is not \"on\" or \"off\""),
            ));
        }
    };
    Ok(Json(result.map_err(failed)?))
}


// ---------------------------------------------------------- the interface --

/// Serves the built interface, or explains why there is not one.
async fn interface(uri: Uri) -> Response {
    let path = uri.path();

    // An API path that got here is a real 404, and must not be answered
    // with an HTML page — a fetch that receives `<!doctype html>` where it
    // expected JSON fails with a parse error that points nowhere.
    if path.starts_with("/api/") {
        return (StatusCode::NOT_FOUND, "no such endpoint").into_response();
    }

    if let Some((bytes, kind)) = assets::file(path) {
        // Vite fingerprints asset filenames, so anything under `/assets/`
        // is safe to cache for a long time; the page itself never is, or an
        // upgrade would keep serving yesterday's interface.
        let cache = if path.starts_with("/assets/") {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        };
        return (
            [(header::CONTENT_TYPE, kind), (header::CACHE_CONTROL, cache)],
            bytes,
        )
            .into_response();
    }

    match assets::index() {
        // Every other path belongs to the router in the browser.
        Some(page) => (
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                (header::CACHE_CONTROL, "no-cache"),
            ],
            page,
        )
            .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            "The interface was not built into this binary.\n\n\
             A release build has it. From a source checkout, run `npm ci && npm run build` in\n\
             the knowlith folder and build again — or run `npm run dev` and use port 5173.\n\n\
             The API on this port works either way.\n",
        )
            .into_response(),
    }
}
