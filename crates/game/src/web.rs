//! Browser build glue (#106). Only compiled for `wasm32`.
//!
//! `localStorage` saves. The network side lives in `net_web.rs`.

use bevy::prelude::*;

/// Read a small saved file (e.g. the character) from `localStorage`.
pub fn load(key: &str) -> Option<String> {
    storage()?.get_item(&storage_key(key)).ok().flatten()
}

/// Save a small file to `localStorage`. Silently does nothing if storage is blocked.
pub fn save(key: &str, value: &str) {
    if let Some(s) = storage()
        && let Err(e) = s.set_item(&storage_key(key), value)
    {
        warn!("could not save {key} to localStorage: {e:?}");
    }
}

fn storage_key(key: &str) -> String {
    format!("friendslop/{key}")
}

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}
