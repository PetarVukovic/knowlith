//! Telling the agent when to reach for the company.
//!
//! A connected MCP server that the agent does not know when to use gets
//! used once, by accident. The server's own `instructions` cover the case
//! where the client passes them through; this covers the case that actually
//! decides whether the product feels alive, which is the agent working in a
//! repository with its own conventions and no reason to think a company's
//! price list is relevant.
//!
//! Both files here belong to the owner and may be hundreds of lines they
//! wrote themselves. So the block is delimited, replaced in place when it
//! already exists, and removable without trace. Nothing outside the markers
//! is ever touched.

use std::fs;
use std::path::PathBuf;

use crate::paths;

const BEGIN: &str = "<!-- knowlith:begin -->";
const END: &str = "<!-- knowlith:end -->";

/// Which agent's standing instructions to write into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Guide {
    /// `~/.codex/AGENTS.md`, read by Codex in every workspace.
    Codex,
    /// `~/.claude/CLAUDE.md`, read by Claude Code in every project.
    ClaudeCode,
}

impl Guide {
    pub fn path(self) -> PathBuf {
        match self {
            Guide::Codex => paths::home().join(".codex").join("AGENTS.md"),
            Guide::ClaudeCode => paths::home().join(".claude").join("CLAUDE.md"),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Guide::Codex => "Codex",
            Guide::ClaudeCode => "Claude Code",
        }
    }
}

/// The block itself.
///
/// Short on purpose. It competes for attention with everything else in the
/// owner's instructions file, and a page of guidance about one MCP server
/// is a page the agent will skim.
pub fn block(company: &str) -> String {
    format!(
        "{BEGIN}\n\
         ## {company}'s own knowledge\n\n\
         The `knowlith` MCP server holds this company's approved rules, prices, procedures and \
         vocabulary. When a question touches how {company} works, it is the source — more \
         authoritative than this file, than the repository, and than anything you already know.\n\n\
         - Begin a real piece of work with `get_relevant_context`, which lists everything the \
           company has decided that touches it. Close with `check_coverage`, which names what you \
           never read. Do not say you checked the company's rules until it comes back clean.\n\
         - Take every figure from `lookup_value`. It reads the row out of the company's own table. \
           Never take a price from a sentence and never interpolate between two you have seen.\n\
         - When something comes back as an open question, say it is open. The owner has not \
           decided it, and it is not yours to decide.\n\
         - Cite the document each answer came from. Every result carries one.\n\n\
         If the server is not running, say so rather than answering from general knowledge.\n\
         {END}"
    )
}

/// Adds or replaces the block, leaving the rest of the file alone.
///
/// Returns the path written.
pub fn write(guide: Guide, company: &str) -> std::io::Result<PathBuf> {
    let path = guide.path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let existing = fs::read_to_string(&path).unwrap_or_default();
    let updated = replace(&existing, &block(company));
    atomically(&path, &updated)?;
    Ok(path)
}

/// Removes the block and nothing else.
pub fn remove(guide: Guide) -> std::io::Result<()> {
    let path = guide.path();
    let Ok(existing) = fs::read_to_string(&path) else {
        return Ok(());
    };
    let stripped = without_block(&existing);
    if stripped != existing {
        atomically(&path, &stripped)?;
    }
    Ok(())
}

/// Whether the owner's instructions already mention us.
pub fn present(guide: Guide) -> bool {
    fs::read_to_string(guide.path())
        .map(|text| text.contains(BEGIN))
        .unwrap_or(false)
}

fn replace(existing: &str, block: &str) -> String {
    let stripped = without_block(existing);
    if stripped.trim().is_empty() {
        return format!("{block}\n");
    }
    format!("{}\n\n{block}\n", stripped.trim_end())
}

/// Everything outside the markers.
///
/// An unterminated block — someone deleted the closing marker by hand — is
/// left exactly where it is rather than swallowing the rest of the file.
fn without_block(text: &str) -> String {
    let Some(start) = text.find(BEGIN) else {
        return text.to_string();
    };
    let Some(end) = text[start..].find(END).map(|offset| start + offset + END.len()) else {
        return text.to_string();
    };
    let mut out = String::with_capacity(text.len());
    out.push_str(text[..start].trim_end());
    let rest = text[end..].trim_start();
    if !rest.is_empty() {
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(rest);
    }
    out
}

fn atomically(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let temporary = path.with_extension(format!("knowlith-{}.tmp", std::process::id()));
    let mut file = fs::File::create(&temporary)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_block_says_the_three_things_that_change_behaviour() {
        let text = block("Termoval d.o.o.");
        assert!(text.contains("get_relevant_context"));
        assert!(text.contains("check_coverage"));
        assert!(text.contains("lookup_value"));
        assert!(text.contains("open question"));
        assert!(text.starts_with(BEGIN) && text.ends_with(END));
    }

    #[test]
    fn writing_into_an_empty_file_produces_only_the_block() {
        let out = replace("", &block("Termoval"));
        assert!(out.starts_with(BEGIN));
        assert_eq!(out.matches(BEGIN).count(), 1);
    }

    #[test]
    fn the_owners_own_instructions_survive() {
        let mine = "# My rules\n\nAlways write tests first.\n";
        let once = replace(mine, &block("Termoval"));
        assert!(once.contains("Always write tests first."));

        // And writing twice does not leave two blocks.
        let twice = replace(&once, &block("Termoval"));
        assert_eq!(twice.matches(BEGIN).count(), 1, "{twice}");
        assert!(twice.contains("Always write tests first."));
    }

    #[test]
    fn a_block_in_the_middle_is_replaced_without_moving_what_is_around_it() {
        let mine = format!("# Top\n\n{}\n\n# Bottom\n", block("Old Co"));
        let updated = replace(&mine, &block("New Co"));
        assert!(updated.contains("# Top"));
        assert!(updated.contains("# Bottom"));
        assert!(updated.contains("New Co"));
        assert!(!updated.contains("Old Co"));
    }

    #[test]
    fn removing_leaves_the_file_as_it_was() {
        let mine = "# My rules\n\nAlways write tests first.\n";
        let with = replace(mine, &block("Termoval"));
        assert_eq!(without_block(&with).trim(), mine.trim());
    }

    #[test]
    fn a_half_deleted_marker_is_not_an_excuse_to_eat_the_file() {
        let damaged = format!("# Mine\n\n{BEGIN}\n## Knowlith\n\n# Still mine\n");
        assert_eq!(without_block(&damaged), damaged);
    }

    #[test]
    fn each_guide_writes_where_its_agent_actually_reads() {
        assert!(Guide::Codex.path().ends_with(".codex/AGENTS.md"));
        assert!(Guide::ClaudeCode.path().ends_with(".claude/CLAUDE.md"));
    }
}
