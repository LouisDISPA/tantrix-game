//! `components/` holds reusable molecules and atoms. They may read `GameCtx`,
//! but session state stays in `pages/` and `state/online_state/`; see
//! `doc/components.md#tiers`. This scan fails when a component names one of
//! the session-side paths below.

use std::fs;
use std::path::{Path, PathBuf};

/// Paths a component must not name, outside comments.
const FORBIDDEN: [&str; 4] = [
    "crate::pages",
    "super::pages",
    "online_state",
    "SessionCommand",
];

/// This file names the forbidden paths itself.
const SELF: &str = "tier_tests.rs";

/// `(line number, forbidden path)` for each forbidden path in `source`,
/// ignoring `//` comments.
fn violations(source: &str) -> Vec<(usize, &'static str)> {
    source
        .lines()
        .enumerate()
        .flat_map(|(index, line)| {
            let code = line.split("//").next().unwrap_or_default();
            FORBIDDEN
                .into_iter()
                .filter(move |path| code.contains(path))
                .map(move |path| (index + 1, path))
        })
        .collect()
}

fn rust_files(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("components directory is readable") {
        let path = entry.expect("directory entry is readable").path();
        if path.is_dir() {
            rust_files(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
}

#[test]
fn components_do_not_import_session_state() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/components");
    let mut files = Vec::new();
    rust_files(&root, &mut files);
    assert!(
        files.len() > 10,
        "found only {} files under {}",
        files.len(),
        root.display()
    );

    let found: Vec<String> = files
        .iter()
        .filter(|path| !path.ends_with(SELF))
        .flat_map(|path| {
            let source = fs::read_to_string(path).expect("component source is readable");
            let shown = path
                .strip_prefix(&root)
                .unwrap_or(path)
                .display()
                .to_string();
            violations(&source)
                .into_iter()
                .map(move |(line, forbidden)| format!("components/{shown}:{line}: {forbidden}"))
                .collect::<Vec<_>>()
        })
        .collect();

    assert!(
        found.is_empty(),
        "components/ must not use session state (doc/components.md#tiers):\n{}",
        found.join("\n")
    );
}

#[test]
fn violations_reports_code_and_skips_comments() {
    let source = "\
use crate::hooks::use_highlight_map;
use crate::pages::game::GameHud;
// A comment may mention online_state.
let cmd = SessionCommand::Leave; // and SessionCommand here
use super::super::pages::online;
";
    assert_eq!(
        violations(source),
        vec![
            (2, "crate::pages"),
            (4, "SessionCommand"),
            (5, "super::pages")
        ]
    );
}
