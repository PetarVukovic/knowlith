//! What is in a folder, before anything is read.
//!
//! The owner is about to point Knowlith at years of their own work, and the
//! honest thing to show first is what was found: how many files, how much of
//! it can actually be read, and what the rest is. A progress bar that starts
//! moving before that question is answered asks for trust it has not earned.
//!
//! Nothing here opens a file. It walks names and sizes, which is fast enough
//! to run while somebody is looking at the screen, and it uses exactly the
//! same two rules the real scan uses — [`is_noise`] and
//! [`DocumentKind::from_extension`] — so the count shown here is the count
//! that will be read.

use std::collections::BTreeMap;
use std::path::Path;

use knowlith_core::DocumentKind;
use serde::{Deserialize, Serialize};

use crate::is_noise;

/// Where the walk gives up.
///
/// A preview is something somebody is waiting on. A folder with more files
/// than this is reported as "more than 20,000", which is a true answer and a
/// fast one; the scan itself has no such limit.
pub const LIMIT: usize = 20_000;

/// A folder, counted but not read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Inventory {
    /// Files found, ignoring the ones that are never worth reading.
    pub files: usize,
    /// Of those, how many are a type that can be read.
    pub readable: usize,
    /// Total size of the readable ones.
    pub bytes: u64,
    /// Readable files by type, largest group first.
    pub types: Vec<TypeCount>,
    /// Why the rest were left out, as reasons rather than a number.
    pub skipped: Vec<Skipped>,
    /// Files that are the same name and the same size as one already seen.
    pub duplicates: usize,
    /// Files whose name says somebody kept the previous version beside the
    /// current one.
    pub old_versions: usize,
    /// When the most recently changed file was last written, RFC 3339.
    pub newest: Option<String>,
    /// True when the walk stopped at [`LIMIT`].
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeCount {
    /// `PDF`, `XLSX`, `DOCX` — what the owner calls it.
    pub label: String,
    pub count: usize,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skipped {
    /// `JPG`, or `no extension`.
    pub label: String,
    pub count: usize,
}

/// Walks a folder and counts what is there.
///
/// Unreadable entries are skipped rather than reported as an error: a folder
/// with one permission-denied subdirectory is still worth previewing, and the
/// scan will report that subdirectory when it reaches it.
pub fn inventory(root: &Path) -> Inventory {
    let mut out = Inventory::default();
    let mut readable: BTreeMap<String, (usize, u64)> = BTreeMap::new();
    let mut skipped: BTreeMap<String, usize> = BTreeMap::new();
    // Name and size together: that pair is what survives a copy between
    // folders, and two genuinely different files almost never share both.
    let mut seen: BTreeMap<(String, u64), usize> = BTreeMap::new();
    let mut newest: Option<std::time::SystemTime> = None;

    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy();
        if is_noise(&name) {
            continue;
        }
        if out.files >= LIMIT {
            out.truncated = true;
            break;
        }
        out.files += 1;

        let metadata = entry.metadata().ok();
        let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);

        let key = (name.to_lowercase(), size);
        let before = seen.entry(key).or_insert(0);
        *before += 1;
        if *before > 1 {
            out.duplicates += 1;
        }

        if looks_like_an_old_version(&name) {
            out.old_versions += 1;
        }

        if let Some(modified) = metadata.as_ref().and_then(|m| m.modified().ok())
            && newest.is_none_or(|current| modified > current)
        {
            newest = Some(modified);
        }

        let ext = entry
            .path()
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default();
        let label = if ext.is_empty() {
            "no extension".to_string()
        } else {
            ext.to_uppercase()
        };

        match DocumentKind::from_extension(ext) {
            Some(_) => {
                let slot = readable.entry(label).or_insert((0, 0));
                slot.0 += 1;
                slot.1 += size;
                out.readable += 1;
                out.bytes += size;
            }
            None => *skipped.entry(label).or_default() += 1,
        }
    }

    out.types = readable
        .into_iter()
        .map(|(label, (count, bytes))| TypeCount { label, count, bytes })
        .collect();
    // Largest group first: the owner wants to see "PDF 210" before "TXT 2".
    out.types.sort_by_key(|t| (std::cmp::Reverse(t.count), t.label.clone()));

    out.skipped = skipped
        .into_iter()
        .map(|(label, count)| Skipped { label, count })
        .collect();
    out.skipped.sort_by_key(|s| (std::cmp::Reverse(s.count), s.label.clone()));

    out.newest = newest.map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339());

    out
}

/// Whether a filename says somebody kept the previous version beside this one.
///
/// Worth getting right in both directions. Missing these means last year's
/// prices come back as current; flagging too eagerly means the owner stops
/// believing the count on the screen. So only two shapes are accepted: a
/// version token followed by digits, and a word that plainly means "not the
/// current one" — in both languages the owner's files are actually named in.
fn looks_like_an_old_version(name: &str) -> bool {
    let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name).to_lowercase();

    // "Cjenik (1).xlsx" — what every operating system calls the second copy.
    if stem.ends_with(')')
        && let Some(open) = stem.rfind('(')
        && stem[open + 1..stem.len() - 1].chars().all(|c| c.is_ascii_digit())
        && stem.len() - open > 2
    {
        return true;
    }

    let words: Vec<&str> = stem
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();

    for (i, word) in words.iter().enumerate() {
        if matches!(
            *word,
            "old" | "stari" | "stara" | "staro" | "kopija" | "copy" | "backup" | "arhiva" | "archive"
        ) {
            return true;
        }
        // "v2", "rev 3", "verzija_4".
        let (prefix, rest) = word.split_at(
            word.find(|c: char| c.is_ascii_digit()).unwrap_or(word.len()),
        );
        if matches!(prefix, "v" | "ver" | "verzija" | "rev") {
            if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) {
                return true;
            }
            // The number is its own word: "cjenik rev 2".
            if rest.is_empty()
                && words.get(i + 1).is_some_and(|n| n.chars().all(|c| c.is_ascii_digit()))
            {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, bytes: &[u8]) {
        std::fs::write(dir.join(name), bytes).unwrap();
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("knowlith-inventory-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn counts_readable_files_by_type() {
        let dir = scratch("by-type");
        write(&dir, "cjenik.csv", b"a,b\n1,2\n");
        write(&dir, "uvjeti.md", b"# Uvjeti\n");
        write(&dir, "biljeske.md", b"# Biljeske\n");

        let found = inventory(&dir);
        assert_eq!(found.files, 3);
        assert_eq!(found.readable, 3);
        // MD has two, CSV one, so MD is reported first.
        assert_eq!(found.types[0].label, "MD");
        assert_eq!(found.types[0].count, 2);
        assert_eq!(found.types[1].label, "CSV");
        assert!(found.bytes > 0);
        assert!(!found.truncated);
    }

    #[test]
    fn a_type_that_cannot_be_read_is_named_not_just_counted() {
        let dir = scratch("skipped");
        write(&dir, "uvjeti.md", b"# Uvjeti\n");
        write(&dir, "logo.jpg", b"\xff\xd8\xff");
        write(&dir, "sken.jpg", b"\xff\xd8\xff");

        let found = inventory(&dir);
        assert_eq!(found.readable, 1);
        assert_eq!(found.skipped, vec![Skipped { label: "JPG".into(), count: 2 }]);
    }

    #[test]
    fn noise_is_not_counted_at_all() {
        let dir = scratch("noise");
        write(&dir, "uvjeti.md", b"# Uvjeti\n");
        write(&dir, "~$uvjeti.docx", b"lock");
        write(&dir, ".DS_Store", b"junk");

        // Not "3 files, 1 readable": the owner never chose to have these, so
        // counting them as skipped would be reporting our own noise back.
        let found = inventory(&dir);
        assert_eq!(found.files, 1);
        assert!(found.skipped.is_empty());
    }

    #[test]
    fn a_copy_kept_beside_the_original_is_counted_once_as_a_duplicate() {
        let dir = scratch("duplicates");
        std::fs::create_dir_all(dir.join("arhiva")).unwrap();
        write(&dir, "cjenik.csv", b"a,b\n1,2\n");
        std::fs::write(dir.join("arhiva").join("cjenik.csv"), b"a,b\n1,2\n").unwrap();

        let found = inventory(&dir);
        assert_eq!(found.files, 2);
        assert_eq!(found.duplicates, 1);
    }

    #[test]
    fn names_that_mean_last_years_copy_are_recognised() {
        for name in [
            "Cjenik v2.xlsx",
            "Cjenik rev 3.xlsx",
            "Cjenik_verzija4.xlsx",
            "Cjenik (1).xlsx",
            "Cjenik - stari.xlsx",
            "Cjenik backup.xlsx",
        ] {
            assert!(looks_like_an_old_version(name), "{name} should look old");
        }
    }

    #[test]
    fn ordinary_names_are_not_mistaken_for_old_copies() {
        // The failure that matters: a current file quietly treated as
        // superseded. "Revizija" is a real word in these documents and
        // "V" is a floor, not a version.
        for name in [
            "Cjenik 2026.xlsx",
            "Revizija procesa.docx",
            "Ugovor V kat.docx",
            "Opci uvjeti.pdf",
            "Ponuda (kopirati prije slanja).docx",
        ] {
            assert!(!looks_like_an_old_version(name), "{name} should not look old");
        }
    }

    #[test]
    fn the_newest_file_is_reported() {
        let dir = scratch("newest");
        write(&dir, "uvjeti.md", b"# Uvjeti\n");
        let found = inventory(&dir);
        assert!(found.newest.is_some());
    }

    #[test]
    fn a_folder_that_is_not_there_is_empty_rather_than_an_error() {
        let found = inventory(Path::new("/definitely/not/here"));
        assert_eq!(found.files, 0);
        assert_eq!(found.readable, 0);
    }
}
