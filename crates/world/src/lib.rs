//! The map, shared by every peer and built the same way on each: terrain
//! heights and paths ([`terrain`]), the village ([`buildings`]), the pond,
//! bonfire and string lights ([`landmarks`]), procedural surface textures
//! ([`surfaces`]) and the fixed places the ground is shaped around
//! ([`places`]). No game code (see "Crates" in AGENTS.md).

pub mod buildings;
pub mod footprints;
pub mod holes;
pub mod landmarks;
pub mod map;
pub mod places;
pub mod surfaces;
pub mod terrain;

// The map was carved out of the game crate; these keep its paths working.
pub(crate) use friendslop_art::art;
pub(crate) use friendslop_core::AppState;

/// The map's data files (loaded at run time, see `friendslop_data`).
pub fn data_files() -> Vec<&'static dyn friendslop_data::DataFile> {
    vec![&places::COMMUNE, &terrain::HEIGHTS]
}
