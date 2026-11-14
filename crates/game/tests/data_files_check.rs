//! Every data file the game reads parses, with every field it needs.
//!
//! Standalone from `tests/it`: the data job runs just this, so it doesn't
//! link the whole game test binary.

#[test]
fn every_data_file_parses() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
    for f in friendslop::data_files() {
        let Ok(bytes) = std::fs::read(root.join(f.path())) else {
            panic!("missing data file {}", f.path());
        };
        assert!(f.set_bytes(&bytes), "{} doesn't parse", f.path());
        // A folder's files (the songs) too.
        for path in f.wanted() {
            let Ok(bytes) = std::fs::read(root.join(&path)) else {
                panic!("missing data file {path}");
            };
            assert!(f.set_file(&path, &bytes), "{path} doesn't parse");
        }
    }
}
