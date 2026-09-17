//! Dependency-aware context packing for agents.
//!
//! One call should carry enough approved knowledge to answer — primary matches,
//! what they rest on, and the quotes behind each claim — without making the
//! agent chase five tool round-trips.

use std::collections::{BTreeMap, BTreeSet};

use knowlith_core::{ContextObject, ObjectKind};
use knowlith_graph::Graph;
use knowlith_lake::{Edge, Lake};

use crate::gate;

/// Cached lake rows for one MCP session.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub revision: String,
    pub objects: Vec<ContextObject>,
    pub edges: Vec<Edge>,
}

impl Snapshot {
    pub fn refresh<'a>(lake: &Lake, previous: &'a mut Option<Self>) -> Result<&'a Self, String> {
        let revision = lake.servable_revision().map_err(|e| e.to_string())?;
        let stale = previous
            .as_ref()
            .map(|cache| cache.revision != revision)
            .unwrap_or(true);
        if stale {
            let objects = lake.objects().map_err(|e| e.to_string())?;
            let edges = lake.edges().map_err(|e| e.to_string())?;
            *previous = Some(Self {
                revision,
                objects,
                edges,
            });
        }
        previous.as_ref().ok_or_else(|| "snapshot missing".into())
    }
}

/// How an object entered the pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Primary,
    Foundation,
}

/// One object chosen for the pack, with graph metadata.
#[derive(Debug, Clone)]
pub struct Packed {
    pub id: String,
    pub role: Role,
    pub why: String,
    pub edge_why: Option<String>,
}

pub struct Plan {
    pub items: Vec<Packed>,
    pub open_questions: Vec<(String, String)>,
    pub stale_titles: Vec<String>,
}

const DEFAULT_DEPTH: usize = 3;
const DEFAULT_MAX: usize = 12;
const MODEL_EDGE_MIN: f32 = 0.7;

/// Builds the set of objects to return for a task question.
pub fn plan(
    lake: &Lake,
    snapshot: &Snapshot,
    question: &str,
    depth: usize,
    max_objects: usize,
) -> Plan {
    let depth = depth.clamp(1, 4);
    let max_objects = max_objects.clamp(1, 16);

    let servable: Vec<&ContextObject> = snapshot
        .objects
        .iter()
        .filter(|o| gate::is_servable(o))
        .collect();

    let graph = Graph::build(&snapshot.edges);
    let edge_meta = edge_lookup(&snapshot.edges);

    // Hybrid discovery: lexical match first, then graph-neighbour boost.
    let mut scores: BTreeMap<String, f32> = BTreeMap::new();
    for id in lake.search_objects(question, 16).unwrap_or_default() {
        *scores.entry(id).or_insert(0.0) += 1.0;
    }
    for object in &servable {
        if overlaps(question, &object.title) || overlaps(question, &object.body) {
            *scores.entry(object.id.clone()).or_insert(0.0) += 0.8;
        }
    }
    for (id, score) in scores.clone() {
        for neighbour in graph_neighbours(&graph, &id) {
            *scores.entry(neighbour).or_insert(0.0) += score * 0.15;
        }
    }

    let mut primaries: Vec<(String, f32)> = scores.into_iter().collect();
    primaries.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    primaries.truncate(8);

    let mut chosen: BTreeMap<String, Packed> = BTreeMap::new();
    for (id, _) in &primaries {
        if let Some(object) = servable.iter().find(|o| o.id == *id) {
            chosen.insert(
                id.clone(),
                Packed {
                    id: id.clone(),
                    role: Role::Primary,
                    why: "matches what you asked".into(),
                    edge_why: None,
                },
            );
            expand_foundations(
                &graph,
                &edge_meta,
                object.id.as_str(),
                depth,
                object.title.as_str(),
                &servable,
                &mut chosen,
            );
        }
    }

    // Foundations before dependents in the output order.
    let mut ordered: Vec<Packed> = chosen.into_values().collect();
    ordered.sort_by_key(|item| match item.role {
        Role::Foundation => 0,
        Role::Primary => 1,
    });
    ordered.truncate(max_objects);

    let open_questions: Vec<(String, String)> = gate::unsettled(&snapshot.objects)
        .into_iter()
        .filter(|(object, _)| {
            primaries.iter().any(|(id, _)| id == &object.id) || overlaps(question, &object.title)
        })
        .map(|(object, why)| (object.title.clone(), why.to_string()))
        .collect();

    let stale_titles: Vec<String> = ordered
        .iter()
        .filter_map(|item| {
            lake.stale_since(&item.id)
                .ok()
                .flatten()
                .map(|_| {
                    servable
                        .iter()
                        .find(|o| o.id == item.id)
                        .map(|o| o.title.clone())
                        .unwrap_or_else(|| item.id.clone())
                })
        })
        .collect();

    Plan {
        items: ordered,
        open_questions,
        stale_titles,
    }
}

fn expand_foundations(
    graph: &Graph,
    edge_meta: &BTreeMap<(String, String), (Option<String>, Option<f32>)>,
    from_id: &str,
    depth: usize,
    under_title: &str,
    servable: &[&ContextObject],
    chosen: &mut BTreeMap<String, Packed>,
) {
    for foundation in graph
        .foundations(from_id)
        .into_iter()
        .filter(|f| f.distance <= depth)
    {
        let key = (from_id.to_string(), foundation.id.clone());
        let (_, confidence) = edge_meta.get(&key).cloned().unwrap_or((None, None));
        if confidence.is_some_and(|c| c < MODEL_EDGE_MIN) {
            continue;
        }
        if let Some(object) = servable.iter().find(|o| o.id == foundation.id) {
            chosen.entry(object.id.clone()).or_insert(Packed {
                id: object.id.clone(),
                role: Role::Foundation,
                why: format!("\"{under_title}\" rests on this"),
                edge_why: edge_meta.get(&key).and_then(|(why, _)| why.clone()),
            });
        }
    }
}

fn edge_lookup(edges: &[Edge]) -> BTreeMap<(String, String), (Option<String>, Option<f32>)> {
    let mut out = BTreeMap::new();
    for edge in edges {
        if edge.kind == knowlith_core::RelationType::DependsOn {
            out.insert(
                (edge.from_id.clone(), edge.to_id.clone()),
                (edge.why.clone(), edge.confidence),
            );
        }
    }
    out
}

fn graph_neighbours(graph: &Graph, id: &str) -> Vec<String> {
    let mut ids = BTreeSet::new();
    for hit in graph.foundations(id) {
        if hit.distance == 1 {
            ids.insert(hit.id);
        }
    }
    for hit in graph.impact(id) {
        if hit.distance == 1 {
            ids.insert(hit.id);
        }
    }
    ids.into_iter().collect()
}

pub fn default_depth() -> usize {
    DEFAULT_DEPTH
}

pub fn default_max_objects() -> usize {
    DEFAULT_MAX
}

fn overlaps(question: &str, title: &str) -> bool {
    let words = |text: &str| -> BTreeSet<String> {
        text.split(|c: char| !c.is_alphanumeric())
            .filter(|word| word.chars().count() >= 4)
            .map(fold)
            .collect()
    };
    !words(question).is_disjoint(&words(title))
}

fn fold(word: &str) -> String {
    word.chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| match c {
            'č' | 'ć' => 'c',
            'ž' => 'z',
            'š' => 's',
            'đ' => 'd',
            other => other,
        })
        .take(5)
        .collect()
}

/// Whether a body likely needs a table lookup alongside it.
pub fn wants_table_lookup(object: &ContextObject) -> bool {
    let haystack = format!("{} {}", object.title, object.body).to_lowercase();
    haystack.contains('€')
        || haystack.contains("eur")
        || haystack.contains("cijena")
        || haystack.contains("price")
        || haystack.contains("cjenik")
        || object.kind == ObjectKind::Term && haystack.contains("iznos")
}
