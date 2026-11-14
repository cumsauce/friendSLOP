//! Snapshots of the game's replicon protocol registrations.
//!
//! Registration order must match on every peer (AGENTS.md), so the ordered
//! list of feature protocol plugins and the resulting replicon [`ProtocolHash`]
//! are both pinned to a snapshot file next to this test. Run with `BLESS=1` to
//! rewrite the snapshots when a registration change is intentional.

use std::fs;
use std::path::PathBuf;

use crate::harness;

/// Shown whenever a snapshot mismatch means "registrations changed".
const FAIL_MSG: &str = "protocol registrations changed; if intentional run BLESS=1 cargo test -p friendslop --test it protocol";

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn snapshot_path(name: &str) -> PathBuf {
    manifest_dir().join("tests/it").join(name)
}

fn features_mod_src() -> String {
    fs::read_to_string(manifest_dir().join("src/features/mod.rs"))
        .expect("src/features/mod.rs must be readable")
}

fn blessed() -> bool {
    std::env::var("BLESS").is_ok_and(|v| v == "1")
}

/// Compare `got` against the snapshot file; with `BLESS=1` rewrite the file.
fn check_snapshot(name: &str, got: &str) {
    let path = snapshot_path(name);
    if blessed() {
        fs::write(&path, format!("{got}\n")).expect("bless: snapshot must be writable");
        return;
    }
    let want = fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!("missing snapshot {}: {e} ({FAIL_MSG})", path.display());
    });
    assert_eq!(want.trim_end(), got, "{FAIL_MSG}");
}

/// The `impl Plugin for FeaturesProtocolPlugin { .. }` body.
fn features_protocol_body(src: &str) -> String {
    let mut body = String::new();
    let mut in_body = false;
    for line in src.lines() {
        if line.contains("impl Plugin for FeaturesProtocolPlugin") {
            in_body = true;
            continue;
        }
        if in_body {
            // The impl block closes with a lone `}` at column 0.
            if line.starts_with('}') {
                break;
            }
            body.push_str(line);
            body.push('\n');
        }
    }
    assert!(!body.is_empty(), "FeaturesProtocolPlugin body not found");
    body
}

/// The `<x>::protocol` / `<x>::protocol_runs` identifier of an
/// `app.add_plugins(..)` line, if that is what the line does.
fn protocol_ident(line: &str) -> Option<&str> {
    let rest = &line[line.find("add_plugins(")? + "add_plugins(".len()..];
    let arg = rest[..rest.find(')')?].trim();
    let last = arg.rsplit("::").next()?;
    // `protocol`, `protocol_runs`, and `mic_protocol`-style suffixed ones all
    // count: every add_plugins in the body registers protocols.
    if last.ends_with("protocol") || last.ends_with("protocol_runs") {
        Some(arg)
    } else {
        None
    }
}

/// Every protocol plugin identifier in FeaturesProtocolPlugin order.
fn protocol_order_names(src: &str) -> Vec<String> {
    features_protocol_body(src)
        .lines()
        .filter_map(protocol_ident)
        .map(str::to_string)
        .collect()
}

#[test]
fn protocol_hash_matches_snapshot() {
    let app = harness::peer();
    let hash = *app
        .world()
        .resource::<bevy_replicon::shared::protocol::ProtocolHash>();
    check_snapshot("protocol_hash.snapshot", &format!("{hash:?}"));
}

#[test]
fn protocol_order_names_match_snapshot() {
    let names = protocol_order_names(&features_mod_src());
    assert!(!names.is_empty(), "no protocol plugins parsed");
    check_snapshot("protocol_order.snapshot", &names.join("\n"));
}

#[test]
fn features_protocol_has_no_cfg_gating() {
    // Every peer must register exactly the same protocols (AGENTS.md: never
    // cfg-gate a replicate/add_*_message/add_*_event), so the ordered list
    // itself may not sit behind `#[cfg(`.
    let body = features_protocol_body(&features_mod_src());
    for (i, line) in body.lines().enumerate() {
        assert!(
            !line.contains("#[cfg("),
            "FeaturesProtocolPlugin body is cfg-gated at body line {}: {}\n{FAIL_MSG}",
            i + 1,
            line.trim()
        );
    }
}
