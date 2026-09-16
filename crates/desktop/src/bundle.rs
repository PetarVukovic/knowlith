//! Knowlith as a Claude Desktop extension.
//!
//! Editing `claude_desktop_config.json` works and is what this crate does
//! for everything else, but it is the developer's path: the owner never
//! sees what was added, the application has to be restarted, and if
//! anything is wrong the only symptom is a server that does not appear.
//!
//! An `.mcpb` bundle is the same server offered the way the platform
//! intends: the owner double-clicks it, Claude Desktop shows what it does
//! and what it may reach, and they press Install. The permissions screen is
//! not an obstacle to route around — for a product whose whole claim is
//! that the company's documents stay on the company's machine, being asked
//! to confirm that is the feature.
//!
//! The binary is copied into the bundle rather than referenced. That costs
//! disk and buys the thing that matters: an extension that keeps working
//! when the owner moves, reinstalls or upgrades the command line underneath
//! it. Regenerating the bundle is how an upgrade reaches Claude Desktop,
//! and the interface offers exactly that.

use std::fs;
use std::io::{Seek, Write};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::paths;

/// What the bundle turned out to be.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bundle {
    pub path: String,
    pub bytes: u64,
    /// The extension's version, which is the Knowlith version it wraps.
    pub version: String,
}

#[derive(Debug, thiserror::Error)]
pub enum BundleError {
    #[error("could not find the knowlith binary to package: {0}")]
    NoBinary(String),
    #[error("could not write the bundle: {0}")]
    Write(#[from] std::io::Error),
    #[error("could not build the bundle: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("could not write the manifest: {0}")]
    Manifest(#[from] serde_json::Error),
}

/// Everything the manifest needs that this crate cannot work out itself.
pub struct Contents<'a> {
    pub company: &'a str,
    /// `(name, description)` for each tool, so the install screen lists what
    /// the extension can do rather than saying "11 tools".
    pub tools: Vec<(String, String)>,
    pub prompts: Vec<Prompt>,
}

/// One prompt, with everything the manifest schema insists on.
///
/// Name and description alone were accepted by the writer here and
/// rejected by Claude Desktop — "Invalid manifest: prompts: Required,
/// Required, Required", one for each of the three. The caller fills this
/// from the gateway's own prompt list so the two cannot disagree.
pub struct Prompt {
    pub name: String,
    pub description: String,
    /// The names a host may substitute into `text`.
    pub arguments: Vec<String>,
    /// The prompt itself, with `${arguments.<name>}` where a value goes.
    pub text: String,
}

/// Writes `~/Knowlith/knowlith.mcpb`.
pub fn build(contents: &Contents<'_>) -> Result<Bundle, BundleError> {
    let binary = std::env::current_exe()
        .map_err(|e| BundleError::NoBinary(e.to_string()))?;
    if !binary.is_file() {
        return Err(BundleError::NoBinary(paths::display(&binary)));
    }

    let root = paths::root();
    fs::create_dir_all(&root)?;
    let target = root.join("knowlith.mcpb");

    // Built beside the destination and renamed, so a half-written bundle is
    // never something the owner can double-click.
    let temporary = root.join(format!("knowlith-{}.mcpb.tmp", std::process::id()));
    let file = fs::File::create(&temporary)?;
    let result = write_archive(file, &binary, contents);

    match result {
        Ok(()) => {
            fs::rename(&temporary, &target)?;
            let bytes = fs::metadata(&target)?.len();
            Ok(Bundle {
                path: paths::display(&target),
                bytes,
                version: env!("CARGO_PKG_VERSION").to_string(),
            })
        }
        Err(e) => {
            let _ = fs::remove_file(&temporary);
            Err(e)
        }
    }
}

fn write_archive<W: Write + Seek>(
    sink: W,
    binary: &Path,
    contents: &Contents<'_>,
) -> Result<(), BundleError> {
    let mut archive = zip::ZipWriter::new(sink);

    let text = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    // Without the executable bit the extension installs and then fails to
    // start, with no message that points at a permission.
    let executable = text.unix_permissions(0o755);

    archive.start_file("manifest.json", text)?;
    archive.write_all(serde_json::to_string_pretty(&manifest(contents))?.as_bytes())?;

    archive.start_file(entry_point(), executable)?;
    let bytes = fs::read(binary)?;
    archive.write_all(&bytes)?;

    // The company's own mark on the install screen, when they gave us one.
    let logo = paths::brand_dir().join("logo.png");
    if logo.is_file() {
        if let Ok(image) = fs::read(&logo) {
            archive.start_file("icon.png", text)?;
            archive.write_all(&image)?;
        }
    }

    archive.finish()?;
    Ok(())
}

/// Where the binary sits inside the bundle.
///
/// Windows needs the extension or the process will not start; the host
/// appends it in some versions and not others, so it is written out.
fn entry_point() -> &'static str {
    if cfg!(windows) {
        "server/knowlith.exe"
    } else {
        "server/knowlith"
    }
}

fn manifest(contents: &Contents<'_>) -> Value {
    let lake = paths::display(&paths::lake_db());
    let has_logo = paths::brand_dir().join("logo.png").is_file();

    let mut manifest = json!({
        "manifest_version": "0.4",
        "name": "knowlith",
        "display_name": format!("{} — company knowledge", contents.company),
        "version": env!("CARGO_PKG_VERSION"),
        "description": format!("{}'s own rules, prices and procedures, as its owner approved them.", contents.company),
        "long_description": long_description(contents.company),
        "author": { "name": "Knowlith" },
        "license": "Apache-2.0",
        "keywords": ["company", "knowledge", "local"],
        "server": {
            "type": "binary",
            "entry_point": entry_point(),
            "mcp_config": {
                // `${__dirname}` is the installed extension's own folder.
                "command": format!("${{__dirname}}/{}", entry_point()),
                "args": ["mcp", "--db", lake, "--company", contents.company],
                "platform_overrides": {
                    "win32": {
                        "command": "${__dirname}/server/knowlith.exe"
                    }
                }
            }
        },
        "tools": contents.tools.iter().map(|(name, description)| json!({
            "name": name,
            "description": description,
        })).collect::<Vec<_>>(),
        // The tool surface is fixed: the same eleven whatever is in the
        // lake, so what is listed here is the whole of it.
        "tools_generated": false,
        "prompts": contents.prompts.iter().map(|prompt| json!({
            "name": prompt.name,
            "description": prompt.description,
            "arguments": prompt.arguments,
            "text": prompt.text,
        })).collect::<Vec<_>>(),
        // The prompt surface is not fixed. Every approved skill becomes a
        // prompt, so this list is the built-ins plus whatever existed the
        // moment the bundle was written. Without this flag the host takes
        // the list for the whole of it, and a skill approved afterwards
        // never reaches the owner — they would have to rebuild and
        // reinstall the extension to see their own procedure.
        "prompts_generated": true,
        "compatibility": {
            "platforms": ["darwin", "win32", "linux"]
        }
    });

    if has_logo {
        manifest["icon"] = json!("icon.png");
    }
    manifest
}

fn long_description(company: &str) -> String {
    format!(
        "Answers questions about {company} from the documents {company} gave Knowlith, and only \
         from the parts its owner has approved.\n\n\
         **Everything stays on this machine.** The extension reads one SQLite file in your \
         Knowlith folder. It opens no network connection, and it has no key, account or server \
         behind it.\n\n\
         **It can write one thing.** When it notices something worth recording it can put a \
         suggestion in your review queue — marked as a suggestion, never as knowledge, and \
         refused outright unless it quotes one of your own documents word for word.\n\n\
         **It says what it does not know.** Where two of your documents disagree, or where you \
         have not approved something yet, it tells the assistant the question is open instead of \
         answering it."
    )
}

/// Opens the bundle the way a double-click would, so Claude Desktop shows
/// its own install screen.
pub fn reveal(bundle: &Bundle) {
    let path = PathBuf::from(&bundle.path);
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg(&path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
    #[cfg(windows)]
    {
        // `cmd /C start` is what associates the file with Claude Desktop;
        // spawning the path directly would try to execute it.
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", &bundle.path])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
    #[cfg(all(not(target_os = "macos"), not(windows)))]
    {
        let _ = std::process::Command::new("xdg-open")
            .arg(&path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contents() -> Contents<'static> {
        Contents {
            company: "Termoval d.o.o.",
            tools: vec![("search_context".into(), "Finds approved rules.".into())],
            prompts: vec![Prompt {
                name: "odobravanje-popusta".into(),
                description: "Odobravanje popusta".into(),
                arguments: vec!["situation".into()],
                text: "Provjeri ${arguments.situation}.".into(),
            }],
        }
    }

    #[test]
    fn the_manifest_names_the_company_and_the_lake() {
        let manifest = manifest(&contents());
        assert_eq!(manifest["manifest_version"], "0.4");
        assert_eq!(manifest["name"], "knowlith");
        assert!(manifest["display_name"].as_str().unwrap().contains("Termoval"));

        let args = manifest["server"]["mcp_config"]["args"].as_array().unwrap();
        assert_eq!(args[0], "mcp");
        assert_eq!(args[1], "--db");
        // An absolute path, because the extension runs from its own folder
        // and a relative one would resolve somewhere nobody intended.
        assert!(Path::new(args[2].as_str().unwrap()).is_absolute());
    }

    #[test]
    fn the_command_points_inside_the_installed_extension() {
        let manifest = manifest(&contents());
        let command = manifest["server"]["mcp_config"]["command"].as_str().unwrap();
        assert!(command.starts_with("${__dirname}/"), "{command}");
        assert_eq!(
            manifest["server"]["entry_point"].as_str().unwrap(),
            command.trim_start_matches("${__dirname}/")
        );
    }

    #[test]
    fn windows_gets_an_executable_with_an_extension() {
        let manifest = manifest(&contents());
        let windows = manifest["server"]["mcp_config"]["platform_overrides"]["win32"]["command"]
            .as_str()
            .unwrap();
        assert!(windows.ends_with(".exe"), "{windows}");
    }

    #[test]
    fn the_install_screen_lists_what_the_extension_can_do() {
        let manifest = manifest(&contents());
        assert_eq!(manifest["tools"][0]["name"], "search_context");
        assert_eq!(manifest["tools_generated"], json!(false));
        // Skills become prompts, so the host has to keep asking. Declared
        // as final, a skill approved after the extension was installed
        // would never appear.
        assert_eq!(manifest["prompts_generated"], json!(true));
        assert_eq!(manifest["prompts"][0]["name"], "odobravanje-popusta");
    }

    /// Claude Desktop refused the whole extension over this: a prompt
    /// with only a name and a description fails validation with
    /// "Invalid manifest: prompts: Required, Required, Required" — one
    /// for each prompt, and no indication of which field is missing.
    #[test]
    fn every_prompt_carries_what_the_manifest_schema_demands() {
        let manifest = manifest(&contents());
        let prompts = manifest["prompts"].as_array().expect("prompts");
        assert!(!prompts.is_empty());
        for prompt in prompts {
            for field in ["name", "description", "arguments", "text"] {
                assert!(
                    prompt.get(field).is_some_and(|v| !v.is_null()),
                    "a prompt went out without {field}: {prompt}"
                );
            }
            assert!(prompt["arguments"].is_array(), "arguments must be a list");
            assert!(
                prompt["text"].as_str().is_some_and(|t| !t.trim().is_empty()),
                "a prompt went out with no text at all"
            );
        }
    }

    #[test]
    fn the_description_is_honest_about_the_one_thing_it_writes() {
        let text = long_description("Termoval");
        assert!(text.contains("stays on this machine"));
        assert!(text.contains("suggestion"));
        assert!(text.contains("open"));
    }

    #[test]
    fn a_bundle_is_a_readable_archive_with_a_manifest_and_a_binary() {
        // Packages the test binary itself, which is a real executable and
        // exactly the shape of the thing that ships.
        let bundle = build(&contents()).expect("the bundle was not written");
        assert!(bundle.bytes > 0);

        let file = fs::File::open(&bundle.path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let names: Vec<String> = archive.file_names().map(str::to_string).collect();
        assert!(names.contains(&"manifest.json".to_string()), "{names:?}");
        assert!(names.contains(&entry_point().to_string()), "{names:?}");

        let mut manifest = String::new();
        {
            use std::io::Read;
            archive
                .by_name("manifest.json")
                .unwrap()
                .read_to_string(&mut manifest)
                .unwrap();
        }
        let parsed: Value = serde_json::from_str(&manifest).unwrap();
        assert_eq!(parsed["name"], "knowlith");

        let _ = fs::remove_file(&bundle.path);
    }
}
