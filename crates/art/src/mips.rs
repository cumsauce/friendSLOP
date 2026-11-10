//! Mipmaps for RGBA8 textures (#227). PNGs load with a single level, so a tree
//! 60 m away samples a 2048² bark texture pixel by pixel: it shimmers, and the
//! GPU reads far more memory than it shows. These are plain functions over
//! pixel bytes: shrink a texture to a size cap, then build its mip chain.
//!
//! Colour is averaged in linear light (sRGB textures don't darken as they
//! shrink), and alpha-tested textures (leaves, grass) keep the same share of
//! pixels above their cutoff on every level, so far foliage doesn't thin out
//! into bare branches.

/// sRGB byte to linear light.
fn lut_to_linear() -> [f32; 256] {
    let mut t = [0.0; 256];
    for (i, v) in t.iter_mut().enumerate() {
        let c = i as f32 / 255.0;
        *v = if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        };
    }
    t
}

/// Linear light (quantised to 4096 steps) back to an sRGB byte.
fn lut_to_srgb() -> Vec<u8> {
    (0..4096)
        .map(|i| {
            let l = i as f32 / 4095.0;
            let c = if l <= 0.003_130_8 {
                l * 12.92
            } else {
                1.055 * l.powf(1.0 / 2.4) - 0.055
            };
            (c * 255.0).round().clamp(0.0, 255.0) as u8
        })
        .collect()
}

struct Luts {
    lin: [f32; 256],
    srgb: Vec<u8>,
}

impl Luts {
    fn new() -> Self {
        Self {
            lin: lut_to_linear(),
            srgb: lut_to_srgb(),
        }
    }
}

/// Halves an RGBA8 image (box filter; an odd edge clamps). Returns the new
/// bytes, width and height (each at least 1).
fn halve(data: &[u8], w: u32, h: u32, srgb: bool, luts: &Luts) -> (Vec<u8>, u32, u32) {
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let mut out = vec![0u8; (nw * nh * 4) as usize];
    let px = |x: u32, y: u32| ((y.min(h - 1) * w + x.min(w - 1)) * 4) as usize;
    for y in 0..nh {
        for x in 0..nw {
            let (x0, y0) = (x * 2, y * 2);
            let taps = [
                px(x0, y0),
                px(x0 + 1, y0),
                px(x0, y0 + 1),
                px(x0 + 1, y0 + 1),
            ];
            let o = ((y * nw + x) * 4) as usize;
            for c in 0..4 {
                if srgb && c < 3 {
                    let l: f32 = taps.iter().map(|&i| luts.lin[data[i + c] as usize]).sum();
                    let q = ((l / 4.0) * 4095.0).round().clamp(0.0, 4095.0) as usize;
                    out[o + c] = luts.srgb[q];
                } else {
                    let s: u32 = taps.iter().map(|&i| data[i + c] as u32).sum();
                    out[o + c] = ((s + 2) / 4) as u8;
                }
            }
        }
    }
    (out, nw, nh)
}

/// The share of pixels whose alpha, scaled by `scale`, reaches `cutoff`.
fn coverage(data: &[u8], cutoff: f32, scale: f32) -> f32 {
    let n = data.len() / 4;
    if n == 0 {
        return 0.0;
    }
    let hit = data
        .chunks_exact(4)
        .filter(|p| (p[3] as f32 / 255.0) * scale >= cutoff)
        .count();
    hit as f32 / n as f32
}

/// Rescales alpha so `coverage(cutoff)` matches `target` (binary search).
fn keep_coverage(data: &mut [u8], cutoff: f32, target: f32) {
    let (mut lo, mut hi) = (0.0f32, 4.0f32);
    for _ in 0..12 {
        let mid = (lo + hi) / 2.0;
        if coverage(data, cutoff, mid) < target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let scale = hi;
    for p in data.chunks_exact_mut(4) {
        p[3] = (p[3] as f32 * scale).round().clamp(0.0, 255.0) as u8;
    }
}

/// Does any pixel have alpha below 255?
pub fn has_alpha(data: &[u8]) -> bool {
    data.chunks_exact(4).any(|p| p[3] < 255)
}

/// How many mip levels a `w`×`h` texture has, down to 1×1.
pub fn level_count(w: u32, h: u32) -> u32 {
    32 - w.max(h).max(1).leading_zeros()
}

/// A texture shrunk by halving until its longer side is at most `max_side`,
/// then followed by its full mip chain. `cutoff` is the alpha-test threshold
/// for masked textures (`None` for opaque or blended ones). Returns the bytes
/// of every level back to back, the top level's size and the level count.
pub fn build(
    data: &[u8],
    w: u32,
    h: u32,
    srgb: bool,
    cutoff: Option<f32>,
    max_side: u32,
) -> (Vec<u8>, u32, u32, u32) {
    let luts = Luts::new();
    let (mut top, mut tw, mut th) = (data.to_vec(), w, h);
    let target = cutoff.map(|c| (c, coverage(data, c, 1.0)));
    while tw.max(th) > max_side.max(1) {
        (top, tw, th) = halve(&top, tw, th, srgb, &luts);
        if let Some((c, t)) = target {
            keep_coverage(&mut top, c, t);
        }
    }
    let levels = level_count(tw, th);
    let mut all = top.clone();
    let (mut cur, mut cw, mut ch) = (top, tw, th);
    for _ in 1..levels {
        (cur, cw, ch) = halve(&cur, cw, ch, srgb, &luts);
        if let Some((c, t)) = target {
            keep_coverage(&mut cur, c, t);
        }
        all.extend_from_slice(&cur);
    }
    (all, tw, th, levels)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, px: [u8; 4]) -> Vec<u8> {
        px.repeat((w * h) as usize)
    }

    #[test]
    fn chain_sizes_add_up() {
        let (all, w, h, levels) = build(&solid(8, 4, [10, 20, 30, 255]), 8, 4, true, None, 64);
        assert_eq!((w, h, levels), (8, 4, 4));
        // 8x4 + 4x2 + 2x1 + 1x1 pixels.
        assert_eq!(all.len(), (32 + 8 + 2 + 1) * 4);
        // A flat colour stays the same colour all the way down.
        assert!(all.chunks_exact(4).all(|p| p == [10, 20, 30, 255]));
    }

    #[test]
    fn big_textures_shrink_to_the_cap() {
        let (all, w, h, levels) = build(&solid(64, 32, [0, 0, 0, 255]), 64, 32, false, None, 16);
        assert_eq!((w, h, levels), (16, 8, 5));
        assert_eq!(all.len(), (128 + 32 + 8 + 2 + 1) * 4);
    }

    #[test]
    fn srgb_averages_in_linear_light() {
        // Black and white average to mid-grey in linear light, which is a
        // bright sRGB value (~188), not 128.
        let mut data = Vec::new();
        for i in 0..4 {
            let v = if i % 2 == 0 { 0 } else { 255 };
            data.extend_from_slice(&[v, v, v, 255]);
        }
        let (all, ..) = build(&data, 2, 2, true, None, 64);
        let mip = &all[16..20];
        assert!((185..=190).contains(&mip[0]), "{mip:?}");
        let (all, ..) = build(&data, 2, 2, false, None, 64);
        assert_eq!(all[16], 128);
    }

    #[test]
    fn masked_foliage_keeps_its_coverage() {
        // Sparse leaves: one pixel in four is solid, the rest clear. A plain
        // box filter makes every mip pixel alpha 64, under a 0.5 cutoff, so
        // the whole tree would vanish in the distance.
        let (w, h) = (16, 16);
        let mut data = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let a = if x % 2 == 0 && y % 2 == 0 { 255 } else { 0 };
                data.extend_from_slice(&[0, 200, 0, a]);
            }
        }
        let (all, ..) = build(&data, w, h, true, Some(0.5), 64);
        let mip1 = &all[(w * h * 4) as usize..][..(8 * 8 * 4) as usize];
        let c = coverage(mip1, 0.5, 1.0);
        assert!(c > 0.2, "coverage {c}");
        let (plain, ..) = build(&data, w, h, true, None, 64);
        let mip1 = &plain[(w * h * 4) as usize..][..(8 * 8 * 4) as usize];
        assert_eq!(coverage(mip1, 0.5, 1.0), 0.0);
    }

    #[test]
    fn level_counts() {
        assert_eq!(level_count(1, 1), 1);
        assert_eq!(level_count(2048, 2048), 12);
        assert_eq!(level_count(1008, 981), 10);
    }
}
