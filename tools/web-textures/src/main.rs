//! Shrink the textures in a web bundle (#191): `web-textures <dir> [max px]`.
//!
//! The nature pack ships 2048x2048 PNGs (about 20 MB). A phone downloads every
//! byte before the world looks right and then holds each one in GPU memory
//! (16 MB+ apiece with mipmaps), which iOS Safari doesn't forgive. The browser
//! build renders at 1x on a small screen, so anything past 512 px is wasted.
//! Rewrites every PNG/JPEG under `dir` bigger than `max` (default 512) in
//! place, keeping its name and aspect ratio, so the glTFs still find them.
//!
//! The modeler's exports point at shared KTX2 files (BC7 / BC5 / BC4, zstd).
//! Desktop GPUs sample those as they are, but phones have no BC formats and
//! Bevy refuses them there, so each `.ktx2` is decoded to a PNG beside it
//! and every `.gltf` that pointed at it is repointed at the PNG.

use std::path::Path;
use std::process::ExitCode;

use image::imageops::FilterType;
use image::{DynamicImage, RgbaImage};

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(dir) = args.next() else {
        eprintln!("usage: web-textures <dir> [max px]");
        return ExitCode::FAILURE;
    };
    let max = args.next().and_then(|s| s.parse().ok()).unwrap_or(512u32);
    let (mut before, mut after) = (0u64, 0u64);
    let mut ktx = 0usize;
    walk(Path::new(&dir), max, &mut before, &mut after, &mut ktx);
    let repointed = repoint(Path::new(&dir));
    println!(
        "web-textures: {:.1} MB -> {:.1} MB ({ktx} KTX2 decoded to PNG, {repointed} glTF(s) repointed)",
        before as f64 / 1e6,
        after as f64 / 1e6
    );
    ExitCode::SUCCESS
}

fn size(p: &Path) -> u64 {
    std::fs::metadata(p).map_or(0, |m| m.len())
}

fn ext(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
}

fn walk(dir: &Path, max: u32, before: &mut u64, after: &mut u64, ktx: &mut usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Baked reflection probes are RGB9E5 cubemaps every GPU samples: keep them.
            if path.file_name().is_some_and(|n| n == "probes") {
                continue;
            }
            walk(&path, max, before, after, ktx);
            continue;
        }
        match ext(&path).as_deref() {
            Some("png" | "jpg" | "jpeg") => {
                *before += size(&path);
                // A missing LFS object is a tiny text pointer: leave it (and
                // anything else that doesn't decode) alone; the game copes.
                if let Ok(img) = image::open(&path)
                    && img.width().max(img.height()) > max
                {
                    let small = img.resize(max, max, FilterType::Triangle);
                    if let Err(e) = small.save(&path) {
                        eprintln!("web-textures: can't write {}: {e}", path.display());
                    }
                }
                *after += size(&path);
            }
            Some("ktx2") => {
                *before += size(&path);
                let png = path.with_extension("png");
                let Ok(bytes) = std::fs::read(&path) else {
                    continue;
                };
                // Pointers and formats we don't know stay as they are.
                let Some(img) = decode_ktx2(&bytes) else {
                    *after += size(&path);
                    continue;
                };
                let img = if img.width().max(img.height()) > max {
                    img.resize(max, max, FilterType::Triangle)
                } else {
                    img
                };
                if let Err(e) = img.save(&png) {
                    eprintln!("web-textures: can't write {}: {e}", png.display());
                    *after += size(&path);
                    continue;
                }
                let _ = std::fs::remove_file(&path);
                *after += size(&png);
                *ktx += 1;
            }
            _ => {}
        }
    }
}

// VkFormat values the modeler writes.
const BC4_UNORM: u32 = 139;
const BC5_UNORM: u32 = 141;
const BC7_UNORM: u32 = 145;
const BC7_SRGB: u32 = 146;

/// Level 0 of a BC4 / BC5 / BC7 KTX2 (optionally zstd-supercompressed) as
/// RGBA8. BC5 is a two-channel normal map, so blue is rebuilt from red and
/// green; BC4 is spread to grey (glTF occlusion reads red).
fn decode_ktx2(bytes: &[u8]) -> Option<DynamicImage> {
    let reader = ktx2::Reader::new(bytes).ok()?;
    let header = reader.header();
    let format = header.format?.value();
    let (w, h) = (header.pixel_width, header.pixel_height.max(1));
    if w % 4 != 0 || h % 4 != 0 {
        return None;
    }
    let level = reader.levels().next()?;
    let data = match header.supercompression_scheme {
        None => level.data.to_vec(),
        Some(ktx2::SupercompressionScheme::Zstandard) => {
            let mut out = vec![0; level.uncompressed_byte_length as usize];
            let n = ruzstd::decoding::FrameDecoder::new()
                .decode_all(level.data, &mut out)
                .ok()?;
            out.truncate(n);
            out
        }
        Some(_) => return None,
    };
    let (block, channels) = match format {
        BC4_UNORM => (8, 1),
        BC5_UNORM => (16, 2),
        BC7_UNORM | BC7_SRGB => (16, 4),
        _ => return None,
    };
    let (bw, bh) = ((w / 4) as usize, (h / 4) as usize);
    if data.len() < bw * bh * block {
        return None;
    }
    let pitch = w as usize * channels;
    let mut texels = vec![0u8; pitch * h as usize];
    for by in 0..bh {
        for bx in 0..bw {
            let src = &data[(by * bw + bx) * block..][..block];
            let dst = &mut texels[by * 4 * pitch + bx * 4 * channels..];
            match format {
                BC4_UNORM => bcdec_rs::bc4(src, dst, pitch, false),
                BC5_UNORM => bcdec_rs::bc5(src, dst, pitch, false),
                _ => bcdec_rs::bc7(src, dst, pitch),
            }
        }
    }
    let rgba: Vec<u8> = texels
        .chunks_exact(channels)
        .flat_map(|t| match channels {
            1 => [t[0], t[0], t[0], 255],
            2 => {
                let (x, y) = (t[0] as f32 / 127.5 - 1.0, t[1] as f32 / 127.5 - 1.0);
                let z = (1.0 - x * x - y * y).max(0.0).sqrt();
                [t[0], t[1], ((z + 1.0) * 127.5).round() as u8, 255]
            }
            _ => [t[0], t[1], t[2], t[3]],
        })
        .collect();
    RgbaImage::from_raw(w, h, rgba).map(DynamicImage::ImageRgba8)
}

/// Point every `.gltf` image at the PNG that replaced its `.ktx2`. Returns
/// how many files changed.
fn repoint(dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut changed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            changed += repoint(&path);
            continue;
        }
        if ext(&path).as_deref() != Some("gltf") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(mut gltf) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let Some(images) = gltf.get_mut("images").and_then(|i| i.as_array_mut()) else {
            continue;
        };
        let base = path.parent().unwrap_or(Path::new("."));
        let mut any = false;
        for image in images {
            let Some(uri) = image.get("uri").and_then(|u| u.as_str()) else {
                continue;
            };
            let Some(stem) = uri.strip_suffix(".ktx2") else {
                continue;
            };
            let png = format!("{stem}.png");
            if !base.join(&png).exists() {
                continue;
            }
            image["uri"] = png.into();
            image["mimeType"] = "image/png".into();
            any = true;
        }
        if any {
            match serde_json::to_string(&gltf) {
                Ok(out) if std::fs::write(&path, &out).is_ok() => changed += 1,
                _ => eprintln!("web-textures: can't rewrite {}", path.display()),
            }
        }
    }
    changed
}
