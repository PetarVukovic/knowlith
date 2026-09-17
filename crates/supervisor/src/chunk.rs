//! Document chunking via chonkie when available, paragraph split otherwise.

use std::io::Write;
use std::process::{Command, Stdio};

/// Splits text into chunks sized for one supervisor context window.
pub fn chunk_text(text: &str, chunk_size: usize) -> Vec<String> {
    if let Ok(chunks) = chunk_via_chonkie(text, chunk_size) {
        if !chunks.is_empty() {
            return chunks;
        }
    }
    fallback_paragraphs(text, chunk_size)
}

fn chunk_via_chonkie(text: &str, chunk_size: usize) -> Result<Vec<String>, String> {
    let script = workspace_script("scripts/chunk_text.py")?;
    let mut child = Command::new("python3")
        .arg(&script)
        .arg(chunk_size.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(text.as_bytes());
    }
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("chonkie script failed".into());
    }
    let body: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;
    Ok(body["chunks"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default())
}

fn workspace_script(relative: &str) -> Result<String, String> {
    if let Ok(root) = std::env::var("KNOWLITH_REPO") {
        let path = std::path::PathBuf::from(root).join(relative);
        if path.is_file() {
            return Ok(path.to_string_lossy().into_owned());
        }
    }
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    for base in [cwd.clone(), cwd.join(".."), cwd.join("../..")] {
        let path = base.join(relative);
        if path.is_file() {
            return Ok(path.to_string_lossy().into_owned());
        }
    }
    Err(format!("could not find {relative}"))
}

fn fallback_paragraphs(text: &str, chunk_size: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    for para in text.split("\n\n") {
        let para = para.trim();
        if para.is_empty() {
            continue;
        }
        if !buf.is_empty() && buf.len() + para.len() > chunk_size {
            out.push(buf.clone());
            buf.clear();
        }
        if !buf.is_empty() {
            buf.push_str("\n\n");
        }
        buf.push_str(para);
    }
    if !buf.is_empty() {
        out.push(buf);
    }
    if out.is_empty() && !text.trim().is_empty() {
        out.push(text.trim().to_string());
    }
    out
}
