//! Shrink the textures in a web bundle (#191): `web-textures <dir> [max px]`.
//!
//! The nature pack ships 2048x2048 PNGs (about 20 MB). A phone downloads every
//! byte before the world looks right and then holds each one in GPU memory
//! (16 MB+ apiece with mipmaps), which iOS Safari doesn't forgive. The browser
//! build renders at 1x on a small screen, so anything past 512 px is wasted.
//! Rewrites every PNG/JPEG under `dir` bigger than `max` (default 512) in
//! place, keeping its name and aspect ratio, so the glTFs still find them.

use std::path::Path;
use std::process::ExitCode;

use image::imageops::FilterType;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(dir) = args.next() else {
        eprintln!("usage: web-textures <dir> [max px]");
        return ExitCode::FAILURE;
    };
    let max = args.next().and_then(|s| s.parse().ok()).unwrap_or(512u32);
    let (mut before, mut after) = (0u64, 0u64);
    walk(Path::new(&dir), max, &mut before, &mut after);
    println!(
        "web-textures: {:.1} MB -> {:.1} MB",
        before as f64 / 1e6,
        after as f64 / 1e6
    );
    ExitCode::SUCCESS
}

fn walk(dir: &Path, max: u32, before: &mut u64, after: &mut u64) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, max, before, after);
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase);
        if !matches!(ext.as_deref(), Some("png" | "jpg" | "jpeg")) {
            continue;
        }
        let size = |p: &Path| std::fs::metadata(p).map_or(0, |m| m.len());
        *before += size(&path);
        // A missing LFS object is a tiny text pointer: leave it (and anything
        // else that doesn't decode) alone; the game already copes.
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
}
