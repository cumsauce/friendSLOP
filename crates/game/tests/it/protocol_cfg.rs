//! Source scan: replicon registrations must never be `cfg`-gated.
//!
//! AGENTS.md: "never `cfg`-gate a `replicate`/`add_*_message`/`add_*_event`" —
//! the browser and desktop peers have to register the identical protocol or
//! they cannot connect. This walks every `.rs` under `crates/*/src` and fails
//! when a line containing `cfg(` sits within [`WINDOW`] lines of a
//! registration line. Escape hatch: put `protocol-cfg-allow` in a comment on
//! either line.

use std::fs;
use std::path::{Path, PathBuf};

/// The registration calls that must stay unconditional.
const REGISTRATIONS: [&str; 5] = [
    "replicate::<",
    "add_client_message",
    "add_server_message",
    "add_client_event",
    "add_server_event",
];

/// Lines before/after a registration that may not contain `cfg(`.
const WINDOW: usize = 3;

/// Comment marker that excuses a registration/`cfg(` pair.
const ALLOW: &str = "protocol-cfg-allow";

fn walk_rust(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_rust(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn workspace_crates_src() -> Vec<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    let Ok(entries) = fs::read_dir(root.join("crates")) else {
        panic!("crates/ must be readable from the workspace root");
    };
    for entry in entries.flatten() {
        let src = entry.path().join("src");
        if src.is_dir() {
            walk_rust(&src, &mut files);
        }
    }
    files.sort();
    files
}

/// `path:line: text` for every `cfg(` too close to a registration in `path`.
fn scan_file(path: &Path) -> Vec<String> {
    let Ok(src) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let lines: Vec<&str> = src.lines().collect();
    let mut bad = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if !REGISTRATIONS.iter().any(|r| line.contains(r)) {
            continue;
        }
        if line.contains(ALLOW) {
            continue;
        }
        let lo = i.saturating_sub(WINDOW);
        let hi = (i + WINDOW + 1).min(lines.len());
        for (j, other) in lines.iter().enumerate().take(hi).skip(lo) {
            if j == i || !other.contains("cfg(") || other.contains(ALLOW) {
                continue;
            }
            bad.push(format!(
                "{}:{}: `{}` within {WINDOW} lines of registration at {} (`{}`)",
                path.display(),
                j + 1,
                other.trim(),
                i + 1,
                line.trim()
            ));
        }
    }
    bad
}

#[test]
fn no_cfg_gated_registrations() {
    let mut bad = Vec::new();
    for file in workspace_crates_src() {
        bad.extend(scan_file(&file));
    }
    assert!(
        bad.is_empty(),
        "cfg-gated replicon registrations (peers would disagree on the protocol; \
         move the registration out of the cfg, or mark the lines `{ALLOW}` if truly fine):\n{}",
        bad.join("\n")
    );
}
