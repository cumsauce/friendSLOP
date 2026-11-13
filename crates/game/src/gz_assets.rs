//! Gzipped models in the browser (#242). Not a friend idea; download size.
//!
//! `scripts/build-web.sh` gzips every model file (`.gltf`, `.bin`: 47 MB ->
//! ~21 MB) and ships `x.bin.gz` instead of `x.bin`. In the browser the asset
//! reader asks for the `.gz` first and inflates it, falling back to the plain
//! file, so a build without the step (or desktop) works unchanged.

use std::path::{Path, PathBuf};

/// The gzipped twin of a model file, if it's a kind the web build packs.
pub fn gz_path(path: &Path) -> Option<PathBuf> {
    let ext = path.extension()?.to_str()?;
    matches!(ext, "gltf" | "bin").then(|| {
        let mut p = path.as_os_str().to_owned();
        p.push(".gz");
        PathBuf::from(p)
    })
}

/// Unpacks a gzip file (no zlib or raw-deflate; `None` if it isn't gzip).
pub fn gunzip(bytes: &[u8]) -> Option<Vec<u8>> {
    const FTEXT_FHCRC: u8 = 0x02;
    const FEXTRA: u8 = 0x04;
    const FNAME: u8 = 0x08;
    const FCOMMENT: u8 = 0x10;
    if bytes.len() < 18 || bytes[0] != 0x1f || bytes[1] != 0x8b || bytes[2] != 8 {
        return None;
    }
    let flags = bytes[3];
    let mut i = 10;
    if flags & FEXTRA != 0 {
        let len = u16::from_le_bytes([*bytes.get(i)?, *bytes.get(i + 1)?]) as usize;
        i += 2 + len;
    }
    for flag in [FNAME, FCOMMENT] {
        if flags & flag != 0 {
            i += bytes.get(i..)?.iter().position(|&b| b == 0)? + 1;
        }
    }
    if flags & FTEXT_FHCRC != 0 {
        i += 2;
    }
    let body = bytes.get(i..bytes.len().checked_sub(8)?)?;
    miniz_oxide::inflate::decompress_to_vec(body).ok()
}

#[cfg(target_arch = "wasm32")]
pub use reader::GzReader;

#[cfg(target_arch = "wasm32")]
mod reader {
    use super::*;
    use bevy::asset::io::{
        AssetReader, AssetReaderError, ErasedAssetReader, PathStream, Reader, VecReader,
    };

    /// Wraps the browser's HTTP reader: `.gz` twins first.
    pub struct GzReader(pub Box<dyn ErasedAssetReader>);

    impl AssetReader for GzReader {
        async fn read<'a>(
            &'a self,
            path: &'a Path,
        ) -> Result<Box<dyn Reader + 'a>, AssetReaderError> {
            if let Some(gz) = gz_path(path)
                && let Ok(mut r) = self.0.read(&gz).await
            {
                let mut bytes = Vec::new();
                if r.read_to_end(&mut bytes).await.is_ok()
                    && let Some(plain) = gunzip(&bytes)
                {
                    return Ok(Box::new(VecReader::new(plain)));
                }
            }
            self.0.read(path).await
        }
        async fn read_meta<'a>(
            &'a self,
            path: &'a Path,
        ) -> Result<Box<dyn Reader + 'a>, AssetReaderError> {
            self.0.read_meta(path).await
        }
        async fn read_directory<'a>(
            &'a self,
            path: &'a Path,
        ) -> Result<Box<PathStream>, AssetReaderError> {
            self.0.read_directory(path).await
        }
        async fn is_directory<'a>(&'a self, path: &'a Path) -> Result<bool, AssetReaderError> {
            self.0.is_directory(path).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_models_have_twins() {
        assert_eq!(
            gz_path(Path::new("models/a.bin")),
            Some(PathBuf::from("models/a.bin.gz"))
        );
        assert_eq!(
            gz_path(Path::new("models/a.gltf")),
            Some(PathBuf::from("models/a.gltf.gz"))
        );
        assert_eq!(gz_path(Path::new("data/a.ron")), None);
    }

    #[test]
    fn gunzips_what_gzip_writes() {
        // `printf 'hello hello hello' | gzip -9n | xxd -i`
        let gz = [
            0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x03, 0xcb, 0x48, 0xcd, 0xc9,
            0xc9, 0x57, 0xc8, 0x40, 0x90, 0x00, 0x80, 0x88, 0xf9, 0xe5, 0x11, 0x00, 0x00, 0x00,
        ];
        assert_eq!(gunzip(&gz).as_deref(), Some(&b"hello hello hello"[..]));
        assert_eq!(gunzip(b"not gzip at all, sorry"), None);
    }
}
