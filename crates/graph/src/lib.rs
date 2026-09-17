//! The knowledge graph.
//!
//! The edges live in SQLite, written in the same transaction as the approval
//! that caused them. This crate lifts them into memory to answer the
//! questions a table cannot answer cheaply: what breaks if I change this, in
//! what order does the change have to be worked through, and does the company
//! have a rule that justifies itself.
//!
//! There is no second database. At the size a company's own knowledge
//! actually reaches — hundreds of objects, not millions — building the graph
//! takes microseconds, and the cost of keeping a separate store honest with
//! the objects it describes is far higher than the cost of rebuilding it.

use std::collections::HashMap;

use knowlith_core::object::RelationType;
use knowlith_lake::Edge;
use petgraph::algo::{kosaraju_scc, toposort};
use petgraph::graph::{DiGraph, NodeIndex};

/// One object that a change reaches, and how far away it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Impact {
    pub id: String,
    /// 1 for something that uses the changed object directly, 2 for something
    /// that uses *that*, and so on. The UI leads with the direct ones because
    /// they are the ones an owner can picture.
    pub distance: usize,
    /// How the change arrives, from the changed object to this one.
    pub path: Vec<String>,
}

pub struct Graph {
    inner: DiGraph<String, RelationType>,
    index: HashMap<String, NodeIndex>,
}

impl Graph {
    /// Builds the graph from the stored edges.
    ///
    /// Only `used_by` and `depends_on` shape it, and `depends_on` is folded
    /// into the reverse `used_by` direction so the graph has one meaning:
    /// **an edge points from a thing to the things that break when it
    /// changes.** `conflicts_with` is deliberately excluded — a disagreement
    /// is not a dependency, and letting it propagate would spread a conflict
    /// through half the company's rules.
    pub fn build(edges: &[Edge]) -> Self {
        let mut inner = DiGraph::new();
        let mut index: HashMap<String, NodeIndex> = HashMap::new();

        let node = |inner: &mut DiGraph<String, RelationType>,
                        index: &mut HashMap<String, NodeIndex>,
                        id: &str| -> NodeIndex {
            *index
                .entry(id.to_string())
                .or_insert_with(|| inner.add_node(id.to_string()))
        };

        for edge in edges {
            let (from, to) = match edge.kind {
                RelationType::UsedBy => (edge.from_id.as_str(), edge.to_id.as_str()),
                RelationType::DependsOn | RelationType::DerivedFrom => {
                    (edge.to_id.as_str(), edge.from_id.as_str())
                }
                RelationType::ConflictsWith => continue,
            };
            let a = node(&mut inner, &mut index, from);
            let b = node(&mut inner, &mut index, to);
            if !inner.edges_connecting(a, b).any(|e| *petgraph::visit::EdgeRef::weight(&e) == edge.kind) {
                inner.add_edge(a, b, edge.kind);
            }
        }

        Self { inner, index }
    }

    pub fn len(&self) -> usize {
        self.inner.node_count()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.node_count() == 0
    }

    pub fn edge_count(&self) -> usize {
        self.inner.edge_count()
    }

    /// Everything a change to `id` reaches, nearest first.
    ///
    /// Breadth-first rather than depth-first on purpose: the first thing the
    /// owner reads should be what directly uses the rule they just changed,
    /// not the far end of the longest chain.
    pub fn impact(&self, id: &str) -> Vec<Impact> {
        let Some(&start) = self.index.get(id) else {
            return Vec::new();
        };

        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::from([start]);
        let mut queue = std::collections::VecDeque::from([(start, vec![id.to_string()])]);

        while let Some((node, path)) = queue.pop_front() {
            for neighbour in self.inner.neighbors(node) {
                if !seen.insert(neighbour) {
                    continue;
                }
                let mut next = path.clone();
                next.push(self.inner[neighbour].clone());
                out.push(Impact {
                    id: self.inner[neighbour].clone(),
                    distance: next.len() - 1,
                    path: next.clone(),
                });
                queue.push_back((neighbour, next));
            }
        }

        out
    }

    /// Everything `id` rests on, nearest first. The mirror of [`Self::impact`].
    pub fn foundations(&self, id: &str) -> Vec<Impact> {
        let Some(&start) = self.index.get(id) else {
            return Vec::new();
        };

        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::from([start]);
        let mut queue = std::collections::VecDeque::from([(start, vec![id.to_string()])]);

        while let Some((node, path)) = queue.pop_front() {
            for neighbour in self
                .inner
                .neighbors_directed(node, petgraph::Direction::Incoming)
            {
                if !seen.insert(neighbour) {
                    continue;
                }
                let mut next = path.clone();
                next.push(self.inner[neighbour].clone());
                out.push(Impact {
                    id: self.inner[neighbour].clone(),
                    distance: next.len() - 1,
                    path: next.clone(),
                });
                queue.push_back((neighbour, next));
            }
        }

        out
    }

    /// The order in which a batch of changes has to be worked through.
    ///
    /// Reviewing a process before the rule it quotes means reviewing it twice,
    /// so the review queue is sorted by this. Returns `None` when the graph
    /// has a cycle, because then no such order exists — call [`Self::cycles`]
    /// to find out which rules refer to each other.
    pub fn propagation_order(&self, ids: &[String]) -> Option<Vec<String>> {
        let sorted = toposort(&self.inner, None).ok()?;
        let wanted: std::collections::HashSet<&str> = ids.iter().map(String::as_str).collect();
        Some(
            sorted
                .into_iter()
                .map(|n| self.inner[n].clone())
                .filter(|id| wanted.contains(id.as_str()))
                .collect(),
        )
    }

    /// Groups of objects that depend on each other in a loop.
    ///
    /// This is a real failure the owner can fix, and it is worth surfacing
    /// rather than tolerating: a rule that justifies itself will confidently
    /// answer a question with its own assumption, and nothing downstream can
    /// tell.
    pub fn cycles(&self) -> Vec<Vec<String>> {
        kosaraju_scc(&self.inner)
            .into_iter()
            .filter(|group| group.len() > 1)
            .map(|group| {
                let mut ids: Vec<String> = group.into_iter().map(|n| self.inner[n].clone()).collect();
                ids.sort();
                ids
            })
            .collect()
    }

    /// Objects nothing else uses.
    ///
    /// Not a problem by itself — a company term stands alone quite properly —
    /// but a *process* nothing reaches is usually one the compiler invented.
    pub fn orphans(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .inner
            .node_indices()
            .filter(|n| {
                self.inner
                    .neighbors_directed(*n, petgraph::Direction::Incoming)
                    .count()
                    == 0
                    && self.inner.neighbors(*n).count() == 0
            })
            .map(|n| self.inner[n].clone())
            .collect();
        ids.sort();
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use knowlith_core::object::RelationOrigin;

    fn edge(from: &str, to: &str, kind: RelationType) -> Edge {
        Edge {
            from_id: from.into(),
            to_id: to.into(),
            kind,
            origin: RelationOrigin::Structural,
            why: None,
            confidence: Some(1.0),
        }
    }

    /// The price list feeds a discount rule, which feeds the quotation
    /// process, which feeds a skill.
    fn chain() -> Graph {
        Graph::build(&[
            edge("fact:price.list", "rule:discount", RelationType::UsedBy),
            edge("rule:discount", "process:quote", RelationType::UsedBy),
            edge("process:quote", "skill:make-offer", RelationType::UsedBy),
        ])
    }

    #[test]
    fn a_price_change_reaches_the_whole_chain_in_order() {
        let hits = chain().impact("fact:price.list");
        assert_eq!(
            hits.iter().map(|h| h.id.as_str()).collect::<Vec<_>>(),
            ["rule:discount", "process:quote", "skill:make-offer"]
        );
        assert_eq!(hits[0].distance, 1);
        assert_eq!(hits[2].distance, 3);
        assert_eq!(
            hits[2].path,
            ["fact:price.list", "rule:discount", "process:quote", "skill:make-offer"]
        );
    }

    #[test]
    fn impact_does_not_run_backwards() {
        assert!(chain().impact("skill:make-offer").is_empty());
    }

    #[test]
    fn foundations_answer_the_opposite_question() {
        let base = chain().foundations("skill:make-offer");
        assert_eq!(
            base.iter().map(|h| h.id.as_str()).collect::<Vec<_>>(),
            ["process:quote", "rule:discount", "fact:price.list"]
        );
    }

    #[test]
    fn depends_on_is_the_same_edge_written_the_other_way() {
        let a = Graph::build(&[edge("rule:x", "fact:y", RelationType::DependsOn)]);
        assert_eq!(a.impact("fact:y").len(), 1);
        assert_eq!(a.impact("fact:y")[0].id, "rule:x");
    }

    #[test]
    fn a_disagreement_is_not_a_dependency() {
        let g = Graph::build(&[edge("rule:a", "rule:b", RelationType::ConflictsWith)]);
        assert!(g.is_empty(), "a conflict must not propagate like a change");
    }

    #[test]
    fn review_order_puts_foundations_first() {
        let order = chain()
            .propagation_order(&[
                "skill:make-offer".into(),
                "fact:price.list".into(),
                "process:quote".into(),
            ])
            .expect("an acyclic graph always has an order");
        assert_eq!(order, ["fact:price.list", "process:quote", "skill:make-offer"]);
    }

    #[test]
    fn a_rule_that_justifies_itself_is_reported() {
        let g = Graph::build(&[
            edge("rule:a", "rule:b", RelationType::UsedBy),
            edge("rule:b", "rule:a", RelationType::UsedBy),
        ]);
        assert_eq!(g.cycles(), vec![vec!["rule:a".to_string(), "rule:b".to_string()]]);
        assert!(g.propagation_order(&["rule:a".into()]).is_none());
    }

    #[test]
    fn the_same_edge_twice_is_still_one_edge() {
        let g = Graph::build(&[
            edge("a", "b", RelationType::UsedBy),
            edge("a", "b", RelationType::UsedBy),
        ]);
        assert_eq!(g.edge_count(), 1);
    }
}
