# friendSLOP patch of rerecast 0.5.0

This is an unmodified copy of the `rerecast` 0.5.0 crate
(https://crates.io/crates/rerecast, MIT OR Apache-2.0 — see `LICENSE-MIT` and
`LICENSE-APACHE`) with one local fix, applied because the stock crate cannot
build a navmesh over our 200x200 m map (see `tests/navmesh.rs`).

## The patch

`src/poly_mesh.rs`, `ContourSet::into_polygon_mesh`: when a single simplified
contour fails ear-clipping triangulation, log a warning and skip that contour
instead of returning `PolygonNavmeshError::InvalidContour` and aborting the
whole bake.

This mirrors upstream C++ recast's `rcBuildPolyMesh`, which does exactly this
(the down-stream `npolys == 0` check already skips empty results), and is the
same fix proposed upstream in https://github.com/janhohenheim/rerecast/pull/60
for https://github.com/janhohenheim/rerecast/issues/59. Without it, every
setting combination we tried fails on our map with "Invalid contour"
(https://github.com/janhohenheim/rerecast/issues/43,
https://github.com/janhohenheim/rerecast/issues/64 — region holes are never
merged into outlines, the root cause, is also unfixed upstream).

Drop this vendored copy (and the `[patch.crates.io]` entry in the workspace
`Cargo.toml`) once upstream releases a version with #60 or #64 fixed.
