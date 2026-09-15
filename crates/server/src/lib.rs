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

pub mod dto;

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use knowlith_core::{ContextObject, Document, ObjectStatus};
use knowlith_lake::Lake;
use serde::Serialize;
use tower_http::cors::{Any, CorsLayer};

use dto::*;

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
        .route("/api/company", get(company))
        .route("/api/objects", get(objects))
        .route("/api/review", get(review))
        .route("/api/review/{id}/approve", post(approve))
        .route("/api/review/{id}/reject", post(reject))
        .route("/api/sources", get(sources))
        .route("/api/skills", get(skills))
        .route("/api/merge-hints", get(merge_hints))
        .route("/api/merge-hints/{keep}/{drop}/merge", post(merge))
        .route("/api/merge-hints/{keep}/{drop}/dismiss", post(dismiss))
        .route("/api/discovery", get(discovery))
        .route("/api/runs", get(runs))
        .route("/api/documents", get(documents))
        .route("/api/tool-reads", get(tool_reads))
        .route("/api/activity", get(activity))
        .route("/api/tree", get(tree))
        .layer(cors)
        .with_state(state)
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
    Ok(Json(Company {
        name: state.company.to_string(),
    }))
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
        file_types.sort_by(|a, b| b.count.cmp(&a.count));

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
            detail: object
                .evidence
                .first()
                .map(|e| format!("From {}, {}.", e.document_id, e.locator))
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
