//! Every mechanic's site (#412, `sites.rs`, `data/base/sites.ron`) is a real
//! place, and code only asks for sites the file has.

use friendslop::sites::{Place, SITES, Sites};

/// The asks that name a site: `sites::awake("nyc")` and friends.
const ASKS: [&str; 5] = ["awake", "here", "arrived", "left", "at"];

fn sites() -> std::sync::Arc<Sites> {
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
    let files: [&dyn friendslop_data::DataFile; 2] = [&SITES, &friendslop_world::places::COMMUNE];
    for file in files {
        let path = file.path();
        let Ok(bytes) = std::fs::read(assets.join(path)) else {
            panic!("missing data file {path}");
        };
        assert!(file.set_bytes(&bytes), "{path} doesn't parse");
    }
    SITES.get()
}

/// The site ids `text` asks for.
fn asked(text: &str) -> Vec<String> {
    let mut ids = Vec::new();
    for ask in ASKS {
        let call = format!("sites::{ask}(\"");
        for (at, _) in text.match_indices(&call) {
            let rest = &text[at + call.len()..];
            if let Some(end) = rest.find('"') {
                ids.push(rest[..end].to_owned());
            }
        }
    }
    ids
}

#[test]
fn every_site_is_somewhere_and_named_once() {
    let list = sites();
    assert!(!list.sites.is_empty());
    for (i, site) in list.sites.iter().enumerate() {
        assert!(
            !list.sites[..i].iter().any(|s| s.id == site.id),
            "two sites are called {:?}",
            site.id
        );
        if let Place::Spot(spot, _) = &site.place {
            assert!(
                friendslop_world::places::spot(spot).is_some(),
                "site {:?} is at the spot {spot:?}, which the commune map doesn't have",
                site.id
            );
        }
    }
}

#[test]
fn code_only_asks_for_sites_the_file_has() {
    let list = sites();
    for (path, text) in super::place_looks::sources() {
        for id in asked(&text) {
            assert!(
                list.sites.iter().any(|s| s.id == id),
                "{path} asks for the site {id:?}, which data/base/sites.ron doesn't have"
            );
        }
    }
}

#[test]
fn asks_are_found_in_source_text() {
    let text = r#".run_if(sites::awake("nyc")); let at = sites::at("crows");"#;
    assert_eq!(asked(text), ["nyc", "crows"]);
}
