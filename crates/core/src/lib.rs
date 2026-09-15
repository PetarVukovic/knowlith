//! Knowlith's domain layer.
//!
//! This crate holds the shape of the product and nothing else: no file access,
//! no database, no network, no model. Everything here is a pure function of its
//! inputs, which is what makes the evidence gate testable on any machine
//! without a provider key.

pub mod document;
pub mod evidence;
pub mod object;

pub use document::{Block, BlockKind, Document, DocumentKind, document_id, sha256_hex};
pub use evidence::{Evidence, Rejection, locate, verify};
pub use object::{
    Confidence, ContextObject, ObjectKind, ObjectStatus, Relation, RelationOrigin, RelationType,
    Subtype,
};
