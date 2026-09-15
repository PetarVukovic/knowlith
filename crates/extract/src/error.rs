//! Why a file was not read.
//!
//! Every variant here is shown to the owner during the scan, so each one has
//! to name something they can recognise in their own folder. "Extraction
//! failed" is not a reason; "this PDF is a scan, with no text layer" is.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ExtractError {
    #[error("this file type is not read")]
    UnsupportedType,
    #[error("the file could not be opened: {0}")]
    Io(#[from] std::io::Error),
    #[error("the file is not readable as text: {0}")]
    Parse(String),
    #[error("this document contains no text, which usually means it is a scan")]
    NoText,
    #[error("the file is empty")]
    Empty,
}

/// One file that was deliberately passed over, with the reason.
#[derive(Debug, Clone)]
pub struct Skipped {
    pub path: PathBuf,
    pub reason: String,
}
