//! Owner-only build projection. This never changes the approved MCP surface.

use std::collections::HashSet;
use axum::{Json, extract::State};
use knowlith_core::{ObjectStatus, RelationType};
use serde_json::{Value, json};
use crate::{ApiResult, AppState, doc_graph_id, failed, kind_str, status_str};

pub(crate) async fn snapshot(State(state): State<AppState>) -> ApiResult<Value> {
    let lake = state.lake.lock().map_err(failed)?;
    let objects = lake.objects().map_err(failed)?;
    let names = lake.document_names().map_err(failed)?;
    let gone = lake.document_gone_map().map_err(failed)?;
    let documents: HashSet<&str> = names.keys().map(String::as_str)
        .filter(|id| !gone.contains_key(*id)).collect();
    let visible: Vec<_> = objects.iter().filter(|o| {
        matches!(o.status, ObjectStatus::Proposed | ObjectStatus::Approved | ObjectStatus::Conflicted)
    }).collect();
    let ids: HashSet<&str> = visible.iter().map(|o| o.id.as_str()).collect();
    let approved = visible.iter().filter(|o| o.status == ObjectStatus::Approved).count();
    let mut nodes: Vec<Value> = visible.iter().map(|o| json!({
        "id": o.id, "title": o.title, "kind": kind_str(o.kind), "status": status_str(o.status),
    })).collect();
    let mut sorted_documents: Vec<_> = documents.iter().copied().collect();
    sorted_documents.sort();
    for id in sorted_documents {
        nodes.push(json!({"id": doc_graph_id(id), "title": names[id], "kind": "document", "status": "extracted"}));
    }
    let mut edges = Vec::new();
    for object in &visible {
        let mut seen = HashSet::new();
        for evidence in &object.evidence {
            if documents.contains(evidence.document_id.as_str()) && seen.insert(&evidence.document_id) {
                edges.push(json!({"from": object.id, "to": doc_graph_id(&evidence.document_id), "type": "quoted_in", "label": "quoted in"}));
            }
        }
    }
    for edge in lake.edges().map_err(failed)? {
        if !ids.contains(edge.from_id.as_str()) || !ids.contains(edge.to_id.as_str()) { continue; }
        let (kind, label) = match edge.kind {
            RelationType::DependsOn => ("depends_on", "needs"),
            RelationType::DerivedFrom => ("derived_from", "comes from"),
            RelationType::UsedBy => ("used_by", "used by"),
            RelationType::ConflictsWith => ("conflicts_with", "disagrees with"),
        };
        edges.push(json!({"from": edge.from_id, "to": edge.to_id, "type": kind, "label": label}));
    }
    // No process discovery here: onboarding polls this while CLI work runs.
    Ok(Json(json!({
        "nodes": nodes, "edges": edges, "assistants": [],
        "counts": {"documents": documents.len(), "discoveries": visible.len() - approved, "approved": approved},
    })))
}
