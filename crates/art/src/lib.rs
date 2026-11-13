//! friendSLOP's look, shared by everything that draws: procedural materials,
//! meshes and textures with small idle animations ([`art`]), texture mipmaps
//! ([`mips`]), baked reflection probes ([`envmap`]), far-mesh LODs ([`lod`]), and the UI
//! palette and widgets ([`theme`]). Depends only on Bevy, never on game code.

pub mod art;
pub mod envmap;
pub mod lod;
pub mod mips;
pub mod palette;
pub mod theme;
