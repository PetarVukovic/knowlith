//! What a source file looks like once Knowlith has read it.
//!
//! Extraction never produces meaning, only structure. A `Document` is a
//! deterministic rendition of one file on disk: the same bytes always yield
//! the same text, the same blocks and the same offsets. Everything downstream
//! — the compiler, the evidence gate, the source preview in the UI — addresses
//! source material through this type and nothing else.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// How a file was parsed. Refined from the extension alone; a spreadsheet that
/// turns out to hold prose is still `Xlsx` here, because this names the parser,
/// not the content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DocumentKind {
    Markdown,
    Text,
    Csv,
    Xlsx,
    Docx,
    Pdf,
}

impl DocumentKind {
    /// Classifies by extension. Returns `None` for anything Knowlith does not
    /// read, which is most of a real folder.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "md" | "markdown" => Some(Self::Markdown),
            "txt" | "text" => Some(Self::Text),
            "csv" | "tsv" => Some(Self::Csv),
            "xlsx" | "xlsm" => Some(Self::Xlsx),
            "docx" => Some(Self::Docx),
            "pdf" => Some(Self::Pdf),
            _ => None,
        }
    }
}

/// The structural role of a block. Deliberately coarse: finer distinctions are
/// the compiler's job, and every one added here is one more thing that has to
/// stay stable across parser versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockKind {
    Heading,
    Paragraph,
    ListItem,
    TableRow,
}

/// One addressable span of a document.
///
/// `start_byte` and `end_byte` are UTF-8 byte offsets into [`Document::text`],
/// never into the file on disk — for a PDF or a spreadsheet those are not the
/// same thing, and pretending otherwise is how an evidence system starts
/// pointing at nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    /// Stable, human-readable address: `"§2.1"`, `"page 4, ¶2"`, `"row 128"`.
    /// This is what the owner sees; it is not parsed by anything.
    pub locator: String,
    pub kind: BlockKind,
    pub text: String,
    pub start_byte: usize,
    pub end_byte: usize,
    /// 1-based, and only ever known for PDFs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    /// Sheet name, for spreadsheets.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sheet: Option<String>,
    /// 1-based row, for spreadsheets and delimited files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row: Option<u32>,
    /// The row split back into columns, so a preview can render a table
    /// instead of a joined string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cells: Option<Vec<String>>,
}

/// One file, read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    /// `doc:` plus the first 16 hex characters of [`Document::sha256`]. Derived
    /// from content, so moving a file does not orphan its evidence.
    pub id: String,
    /// Absolute path at the time of reading. Provenance, not identity.
    pub path: String,
    pub name: String,
    pub kind: DocumentKind,
    /// Size of the file on disk.
    pub byte_len: u64,
    /// Hash of the file on disk. Drives incremental sync.
    pub sha256: String,
    /// The deterministic text rendition every offset refers to.
    pub text: String,
    /// Hash of the rendition. A parser change moves this even when the file has
    /// not changed, which is exactly when stored evidence must be rechecked.
    pub text_sha256: String,
    /// True when the rendition is byte-identical to the file on disk, so a
    /// block offset is also an offset into the owner's own file. False for
    /// PDF, DOCX and spreadsheets, where the file is a container and the text
    /// only exists once a parser has produced it.
    pub verbatim: bool,
    /// Last modification time of the file, RFC 3339.
    pub modified: String,
    /// Column headers, when the file is tabular.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub columns: Option<Vec<String>>,
    pub blocks: Vec<Block>,
}

impl Document {
    /// The block whose span contains `offset`, if any.
    pub fn block_at(&self, offset: usize) -> Option<&Block> {
        self.blocks
            .iter()
            .find(|b| offset >= b.start_byte && offset < b.end_byte)
    }

    pub fn block(&self, locator: &str) -> Option<&Block> {
        self.blocks.iter().find(|b| b.locator == locator)
    }
}

/// `doc:` plus the first 16 hex characters of the content hash.
pub fn document_id(sha256: &str) -> String {
    format!("doc:{}", &sha256[..16.min(sha256.len())])
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}
