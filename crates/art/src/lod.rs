//! Cheaper stand-ins for far meshes (#227, WeiYa: "use ... lods"). Plain
//! functions over vertex positions and triangle indices, so they're cheap to
//! test and need no GPU:
//!
//! - [`cluster`]: solid shapes (tree trunks, branches) are simplified by
//!   snapping vertices to a grid and keeping one per cell, dropping the
//!   triangles that collapse. Vertices facing different ways never merge, so
//!   the shading keeps its shape.
//! - [`thin_cards`]: foliage made of many small loose cards (leaf quads)
//!   keeps one card in N, each grown so the canopy covers the same area.
//!
//! [`simplify`] picks the right one for a mesh.

use std::collections::HashMap;

/// A simplified mesh: for each new vertex, the old vertex it copies (so the
/// caller can carry over normals, UVs and colours), new positions, and new
/// triangle indices.
#[derive(Debug, Clone, PartialEq)]
pub struct Lod {
    pub source: Vec<u32>,
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn bounds(positions: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for p in positions {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    (lo, hi)
}

/// The length of a mesh's bounding box diagonal.
pub fn diagonal(positions: &[[f32; 3]]) -> f32 {
    if positions.is_empty() {
        return 0.0;
    }
    let (lo, hi) = bounds(positions);
    let d = sub(hi, lo);
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

/// Groups triangles that share vertex indices. Returns, per triangle, its
/// group number, and the number of groups.
fn components(indices: &[u32], vertices: usize) -> (Vec<usize>, usize) {
    let mut parent: Vec<usize> = (0..vertices).collect();
    fn find(p: &mut [usize], mut x: usize) -> usize {
        while p[x] != x {
            p[x] = p[p[x]];
            x = p[x];
        }
        x
    }
    for t in indices.chunks_exact(3) {
        let a = find(&mut parent, t[0] as usize);
        for &v in &t[1..] {
            let b = find(&mut parent, v as usize);
            parent[b] = a;
        }
    }
    let mut ids = HashMap::new();
    let tri_group = indices
        .chunks_exact(3)
        .map(|t| {
            let root = find(&mut parent, t[0] as usize);
            let n = ids.len();
            *ids.entry(root).or_insert(n)
        })
        .collect();
    (tri_group, ids.len())
}

/// Vertex clustering: one vertex per `cell`-sized grid cell (and per rough
/// facing, from `normals`), keeping the original vertex nearest the cell's
/// average so attributes stay exact. Collapsed and repeated triangles go.
pub fn cluster(
    positions: &[[f32; 3]],
    normals: Option<&[[f32; 3]]>,
    indices: &[u32],
    cell: f32,
) -> Lod {
    let cell = cell.max(1e-6);
    let key = |i: usize| {
        let p = positions[i];
        let c = [
            (p[0] / cell).floor() as i32,
            (p[1] / cell).floor() as i32,
            (p[2] / cell).floor() as i32,
        ];
        // Six facing buckets: the normal's dominant axis and its sign.
        let facing = normals.and_then(|n| n.get(i)).map_or(0u8, |n| {
            let (ax, v) = (0..3)
                .map(|k| (k as u8, n[k]))
                .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
                .unwrap_or((0, 0.0));
            ax * 2 + u8::from(v < 0.0)
        });
        (c, facing)
    };
    // Cell sums, to find each cell's average.
    let mut cells: HashMap<([i32; 3], u8), ([f32; 3], u32)> = HashMap::new();
    for i in 0..positions.len() {
        let e = cells.entry(key(i)).or_insert(([0.0; 3], 0));
        for k in 0..3 {
            e.0[k] += positions[i][k];
        }
        e.1 += 1;
    }
    // Each cell's representative: the vertex nearest its average.
    let mut rep: HashMap<([i32; 3], u8), (u32, f32)> = HashMap::new();
    for i in 0..positions.len() {
        let k = key(i);
        let (sum, n) = cells[&k];
        let avg = [sum[0] / n as f32, sum[1] / n as f32, sum[2] / n as f32];
        let d = sub(positions[i], avg);
        let dist = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
        let e = rep.entry(k).or_insert((i as u32, f32::MAX));
        if dist < e.1 {
            *e = (i as u32, dist);
        }
    }
    let mut new_index: HashMap<u32, u32> = HashMap::new();
    let mut lod = Lod {
        source: Vec::new(),
        positions: Vec::new(),
        indices: Vec::new(),
    };
    let mut seen = std::collections::HashSet::new();
    for t in indices.chunks_exact(3) {
        let r: Vec<u32> = t.iter().map(|&v| rep[&key(v as usize)].0).collect();
        if r[0] == r[1] || r[1] == r[2] || r[0] == r[2] {
            continue;
        }
        // The same triangle twice (either winding is a different face).
        let mut sorted = [r[0], r[1], r[2]];
        let start = (0..3).min_by_key(|&k| sorted[k]).unwrap_or(0);
        sorted.rotate_left(start);
        if !seen.insert(sorted) {
            continue;
        }
        for v in r {
            let n = *new_index.entry(v).or_insert_with(|| {
                lod.source.push(v);
                lod.positions.push(positions[v as usize]);
                (lod.source.len() - 1) as u32
            });
            lod.indices.push(n);
        }
    }
    lod
}

/// Keeps every `keep_one_in`-th loose card (a group of triangles sharing
/// vertices), each scaled about its centre so the total area stays the same.
pub fn thin_cards(positions: &[[f32; 3]], indices: &[u32], keep_one_in: usize) -> Lod {
    let keep_one_in = keep_one_in.max(1);
    let (tri_group, groups) = components(indices, positions.len());
    // Centre of each card.
    let mut centre = vec![([0.0f32; 3], 0u32); groups];
    let mut counted = vec![false; positions.len()];
    for (t, &g) in indices.chunks_exact(3).zip(&tri_group) {
        for &v in t {
            if !std::mem::replace(&mut counted[v as usize], true) {
                for k in 0..3 {
                    centre[g].0[k] += positions[v as usize][k];
                }
                centre[g].1 += 1;
            }
        }
    }
    // Growing by sqrt(n) keeps the total card area, but fewer cards overlap
    // less, so that reads as a fatter canopy; a gentler growth matches the
    // silhouette.
    let grow = (keep_one_in as f32).powf(GROW_POWER);
    let mut new_index: HashMap<u32, u32> = HashMap::new();
    let mut lod = Lod {
        source: Vec::new(),
        positions: Vec::new(),
        indices: Vec::new(),
    };
    for (t, &g) in indices.chunks_exact(3).zip(&tri_group) {
        if g % keep_one_in != 0 {
            continue;
        }
        let (sum, n) = centre[g];
        let c = [sum[0] / n as f32, sum[1] / n as f32, sum[2] / n as f32];
        for &v in t {
            let n = *new_index.entry(v).or_insert_with(|| {
                let p = positions[v as usize];
                let d = sub(p, c);
                lod.source.push(v);
                lod.positions
                    .push([c[0] + d[0] * grow, c[1] + d[1] * grow, c[2] + d[2] * grow]);
                (lod.source.len() - 1) as u32
            });
            lod.indices.push(n);
        }
    }
    lod
}

/// Kept cards grow by `keep_one_in ^ GROW_POWER`.
const GROW_POWER: f32 = 0.3;
/// Loose cards have at most this many triangles each (a leaf quad is 2).
const CARD_TRIS: f32 = 4.0;
/// Up to this many triangles per piece (a pine needle fan is ~22) a mesh is
/// neither loose cards nor solid: thinning would change its shape and
/// clustering would shred it, so it gets no far version.
const FAN_TRIS: f32 = 24.0;

/// A far version of a mesh: loose foliage cards thinned to one in
/// `keep_one_in`, solid shapes clustered at 1/`detail` of their size. `None`
/// when it wouldn't save at least a third of the triangles (not worth a
/// second mesh).
pub fn simplify(
    positions: &[[f32; 3]],
    normals: Option<&[[f32; 3]]>,
    indices: &[u32],
    detail: f32,
    keep_one_in: usize,
) -> Option<Lod> {
    let tris = indices.len() / 3;
    if tris < 64 {
        return None;
    }
    let (_, groups) = components(indices, positions.len());
    let per_piece = tris as f32 / groups.max(1) as f32;
    let lod = if per_piece <= CARD_TRIS {
        thin_cards(positions, indices, keep_one_in)
    } else if per_piece <= FAN_TRIS {
        return None;
    } else {
        cluster(positions, normals, indices, diagonal(positions) / detail)
    };
    (lod.indices.len() / 3 <= tris * 2 / 3 && !lod.indices.is_empty()).then_some(lod)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `n`×`n` grid of quads in the XZ plane, `size` across, facing up.
    fn sheet(n: u32, size: f32) -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<u32>) {
        let mut p = Vec::new();
        for z in 0..=n {
            for x in 0..=n {
                p.push([x as f32 / n as f32 * size, 0.0, z as f32 / n as f32 * size]);
            }
        }
        let mut idx = Vec::new();
        let w = n + 1;
        for z in 0..n {
            for x in 0..n {
                let a = z * w + x;
                idx.extend_from_slice(&[a, a + w, a + 1, a + 1, a + w, a + w + 1]);
            }
        }
        let normals = vec![[0.0, 1.0, 0.0]; p.len()];
        (p, normals, idx)
    }

    /// `n` separate square cards, 1 m apart, each `s` wide.
    fn cards(n: u32, s: f32) -> (Vec<[f32; 3]>, Vec<u32>) {
        let mut p = Vec::new();
        let mut idx = Vec::new();
        for i in 0..n {
            let x = i as f32;
            let b = p.len() as u32;
            p.extend_from_slice(&[
                [x, 0.0, 0.0],
                [x + s, 0.0, 0.0],
                [x, s, 0.0],
                [x + s, s, 0.0],
            ]);
            idx.extend_from_slice(&[b, b + 1, b + 2, b + 1, b + 3, b + 2]);
        }
        (p, idx)
    }

    fn area(p: &[[f32; 3]], idx: &[u32]) -> f32 {
        idx.chunks_exact(3)
            .map(|t| {
                let (a, b, c) = (p[t[0] as usize], p[t[1] as usize], p[t[2] as usize]);
                let (u, v) = (sub(b, a), sub(c, a));
                let x = [
                    u[1] * v[2] - u[2] * v[1],
                    u[2] * v[0] - u[0] * v[2],
                    u[0] * v[1] - u[1] * v[0],
                ];
                (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt() / 2.0
            })
            .sum()
    }

    #[test]
    fn clustering_cuts_triangles_and_keeps_the_shape() {
        let (p, n, idx) = sheet(32, 4.0);
        let lod = cluster(&p, Some(&n), &idx, 0.5);
        assert!(lod.indices.len() < idx.len() / 4, "{}", lod.indices.len());
        assert!(!lod.indices.is_empty());
        // Every kept vertex is an original one, unmoved.
        for (i, &s) in lod.source.iter().enumerate() {
            assert_eq!(lod.positions[i], p[s as usize]);
        }
        // Still roughly the same sheet.
        let a = area(&lod.positions, &lod.indices);
        assert!((a - 16.0).abs() < 4.0, "area {a}");
    }

    #[test]
    fn opposite_faces_never_merge() {
        // Two coincident sheets facing opposite ways stay two sheets.
        let (p, n, idx) = sheet(8, 2.0);
        let mut p2 = p.clone();
        p2.extend_from_slice(&p);
        let mut n2 = n.clone();
        n2.extend(n.iter().map(|_| [0.0, -1.0, 0.0]));
        let mut idx2 = idx.clone();
        let off = p.len() as u32;
        idx2.extend(
            idx.chunks_exact(3)
                .flat_map(|t| [t[0] + off, t[2] + off, t[1] + off]),
        );
        let lod = cluster(&p2, Some(&n2), &idx2, 0.5);
        let down = lod.source.iter().filter(|&&s| s >= off).count();
        assert!(down > 0 && down < lod.source.len());
    }

    #[test]
    fn thinning_halves_cards_and_grows_the_rest() {
        let (p, idx) = cards(100, 0.2);
        let lod = thin_cards(&p, &idx, 2);
        assert_eq!(lod.indices.len(), idx.len() / 2);
        // Half the cards, each a bit bigger: between half and all the area.
        let (a0, a1) = (area(&p, &idx), area(&lod.positions, &lod.indices));
        assert!(a1 > a0 * 0.6 && a1 < a0, "{a0} vs {a1}");
    }

    #[test]
    fn needle_fans_are_left_alone() {
        // Pieces of 8 triangles: too chunky to thin, too thin to cluster.
        let (p, idx) = cards(100, 0.2);
        let mut fans = Vec::new();
        for q in idx.chunks_exact(6).collect::<Vec<_>>().chunks(4) {
            for (k, quad) in q.iter().enumerate() {
                // Join each group of four cards through their first vertex.
                let first = q[0][0];
                fans.extend(
                    quad.iter()
                        .map(|&v| if k > 0 && v == quad[0] { first } else { v }),
                );
            }
        }
        assert_eq!(simplify(&p, None, &fans, 40.0, 2), None);
    }

    #[test]
    fn simplify_picks_the_method_and_skips_tiny_meshes() {
        let (p, idx) = cards(100, 0.2);
        let lod = simplify(&p, None, &idx, 40.0, 2).map(|l| l.indices.len());
        assert_eq!(lod, Some(idx.len() / 2));
        let (p, n, idx) = sheet(32, 4.0);
        assert!(simplify(&p, Some(&n), &idx, 12.0, 2).is_some());
        let (p, n, idx) = sheet(4, 4.0);
        assert_eq!(simplify(&p, Some(&n), &idx, 12.0, 2), None);
    }
}
