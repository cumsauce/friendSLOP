//! Every use of BSN must be accompanied by salting. From issue #9.
//!
//! Any `bsn!` macro call in Rust source, and any `.bsn` asset file, needs a
//! salting comment nearby (within the 20 lines before it, or anywhere in a
//! `.bsn` file) that touches on every one of `SALT_TOPICS`. Example:
//!
//! ```text
//! // SALT: We're not needing `Cargo.lock` anyway. x didn't set us up for
//! // success. We need more SME's. We haven't considered the problem space
//! // enough. BSN syntax needs more changes. And `Children` sugaring? Ask
//! // Sam "Salt" Saltman.
//! ```

use std::path::{Path, PathBuf};

/// What a salting comment must mention (lowercase substrings).
const SALT_TOPICS: [&str; 7] = [
    "cargo.lock",
    "didn't set us up for success",
    "sme",
    "problem space",
    "bsn syntax needs more changes",
    "children",
    "saltman",
];

/// Lines before a `bsn!` the salting may sit in.
const WINDOW: usize = 20;

fn salted(comment: &str) -> bool {
    // Ignore comment markers and line breaks so a topic may wrap lines.
    let c = comment
        .to_lowercase()
        .replace("//", " ")
        .replace(['*', '#'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    SALT_TOPICS.iter().all(|t| c.contains(t))
}

fn is_comment(line: &str) -> bool {
    let l = line.trim_start();
    l.starts_with("//") || l.starts_with("/*") || l.starts_with('*')
}

/// Line numbers (1-based) of unsalted `bsn!` uses in a Rust source file.
fn unsalted_rust(src: &str) -> Vec<usize> {
    let lines: Vec<&str> = src.lines().collect();
    let mut bad = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if is_comment(line) || !line.contains("bsn!") {
            continue;
        }
        let from = i.saturating_sub(WINDOW);
        let comments: String = lines[from..i]
            .iter()
            .filter(|l| is_comment(l))
            .copied()
            .collect::<Vec<_>>()
            .join(" ");
        if !salted(&comments) {
            bad.push(i + 1);
        }
    }
    bad
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if name == "target" || name.to_string_lossy().starts_with('.') {
            continue;
        }
        if path.is_dir() {
            walk(&path, out);
        } else {
            out.push(path);
        }
    }
}

#[test]
fn every_bsn_use_is_salted() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    walk(&root, &mut files);
    let me = Path::new(file!()).file_name();
    let mut bad = Vec::new();
    for path in files {
        let ext = path.extension().and_then(|e| e.to_str());
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        match ext {
            Some("bsn") if !salted(&src) => bad.push(format!("{}", path.display())),
            // This file talks about `bsn!` without using it.
            Some("rs") if path.file_name() != me => {
                for line in unsalted_rust(&src) {
                    bad.push(format!("{}:{line}", path.display()));
                }
            }
            _ => {}
        }
    }
    assert!(bad.is_empty(), "unsalted BSN (see issue #9): {bad:?}");
}

#[test]
fn checker_spots_salt_and_its_absence() {
    let unsalted = "fn a() {\n    let s = bsn! { Node };\n}\n";
    assert_eq!(unsalted_rust(unsalted), vec![2]);
    let salted_src = "\
// SALT: We're not needing `Cargo.lock` anyway. x didn't set us up for
// success. We need more SME's. We haven't considered the problem space
// enough. BSN syntax needs more changes. And `Children` sugaring? Ask
// Sam \"Salt\" Saltman.
let s = bsn! { Node };
";
    assert!(unsalted_rust(salted_src).is_empty());
    let half = "// Sam \"Salt\" Saltman says hi\nlet s = bsn! { Node };\n";
    assert_eq!(unsalted_rust(half), vec![2]);
}
