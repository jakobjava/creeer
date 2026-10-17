//! Clipping and caps on the real planet (`clip.rs`, `caps.rs`): a chunk with its caps is a
//! closed solid, neighbours of one level share their borders bit for bit, and between levels
//! no ray gets from the air into the ground without meeting the surface or a cap.

use crate::chunk::ChunkKey;
use crate::elevation::fibonacci_sphere;
use crate::surface_nets::WorldMesh;
use crate::{Planet, Terrain};
use creeer_math::glam::{DVec3, IVec3};
use std::collections::HashMap;
use std::sync::OnceLock;

fn terrain() -> &'static Terrain {
    static TERRAIN: OnceLock<Terrain> = OnceLock::new();
    TERRAIN.get_or_init(|| {
        Terrain::new(Planet::load(&crate::data_path("planets/home.ron")).expect("home.ron"))
    })
}

fn world(key: ChunkKey) -> Option<WorldMesh> {
    terrain().mesh_world(key).map(|(_, w)| w)
}

/// Points of the surface on land: `level` of them where a face of the chunk grid lies within
/// 15° of level ground, then as many elsewhere.
fn spots(count: usize) -> Vec<DVec3> {
    let t = terrain();
    let land: Vec<DVec3> = fibonacci_sphere(3000)
        .filter(|&d| (50.0..2500.0).contains(&t.surface_height(d)))
        .collect();
    let flat_face = |d: &DVec3| d.abs().max_element() > creeer_math::det::cos(15f64.to_radians());
    let (level, other): (Vec<DVec3>, Vec<DVec3>) = land.into_iter().partition(flat_face);
    let at = |d: DVec3| d * (t.planet().datum.radius(d) + t.surface_height(d));
    level
        .into_iter()
        .take(count)
        .chain(other.into_iter().take(count))
        .map(at)
        .collect()
}

fn bits(p: DVec3) -> [u64; 3] {
    p.to_array().map(f64::to_bits)
}

/// Directed edges of `indices` (welded by exact position) not matched by a reversed edge.
fn unmatched(w: &WorldMesh, indices: &[u32]) -> usize {
    let mut count: HashMap<([u64; 3], [u64; 3]), i64> = HashMap::new();
    for t in indices.as_chunks::<3>().0 {
        for k in 0..3 {
            let (a, b) = (w.v.world[t[k] as usize], w.v.world[t[(k + 1) % 3] as usize]);
            if bits(a) != bits(b) {
                *count.entry((bits(a), bits(b))).or_default() += 1;
            }
        }
    }
    count
        .iter()
        .filter(|&(&(a, b), &n)| count.get(&(b, a)).copied().unwrap_or(0) != n)
        .count()
}

#[test]
fn a_chunk_with_its_caps_is_a_closed_solid() {
    let mut checked = 0;
    for p in spots(4) {
        for level in [0u8, 2, 4] {
            let key = ChunkKey::containing(p, level);
            let Some(w) = world(key) else { continue };
            let (lo, hi) = (key.origin(), key.origin() + DVec3::splat(key.size()));
            for &q in &w.v.world[..w.surface_vertices] {
                assert!(
                    q.cmpge(lo).all() && q.cmple(hi).all(),
                    "{key:?}: vertex outside"
                );
            }
            assert_eq!(
                unmatched(&w, &w.indices),
                0,
                "{key:?}: surface and caps not closed"
            );
            // Closed and wound outwards: it encloses a positive volume, less than the box.
            let volume: f64 = w
                .indices
                .as_chunks::<3>()
                .0
                .iter()
                .map(|t| {
                    let [a, b, c] = t.map(|i| w.v.world[i as usize] - lo);
                    a.dot(b.cross(c)) / 6.0
                })
                .sum();
            assert!(
                volume > 0.0 && volume < key.size() * key.size() * key.size(),
                "{key:?}: volume {volume}"
            );
            checked += 1;
        }
    }
    assert!(checked >= 12, "only {checked} chunks");
}

#[test]
fn neighbours_of_one_level_share_their_borders_bit_for_bit() {
    let mut shared = 0;
    for p in spots(3) {
        let a = ChunkKey::containing(p, 2);
        for axis in 0..3 {
            let mut step = IVec3::ZERO;
            step[axis] = 1;
            let b = ChunkKey {
                pos: a.pos + step,
                ..a
            };
            let (Some(wa), Some(wb)) = (world(a), world(b)) else {
                continue;
            };
            // Surface edges lying on the common face: a's upper face, b's lower one.
            let on = |w: &WorldMesh, face: usize| {
                let mut edges = Vec::new();
                for t in w.indices[..w.surface_indices].as_chunks::<3>().0 {
                    for k in 0..3 {
                        let (i, j) = (t[k] as usize, t[(k + 1) % 3] as usize);
                        if w.v.planes[i] & w.v.planes[j] & (1 << face) != 0 {
                            edges.push((bits(w.v.world[i]), bits(w.v.world[j])));
                        }
                    }
                }
                edges.sort_unstable();
                edges
            };
            let from_a = on(&wa, 2 * axis + 1);
            let mut from_b: Vec<_> = on(&wb, 2 * axis).into_iter().map(|(i, j)| (j, i)).collect();
            from_b.sort_unstable();
            assert_eq!(from_a, from_b, "{a:?} and {b:?} along axis {axis}");
            shared += from_a.len();
        }
    }
    assert!(shared > 100, "only {shared} border edges compared");
}

/// Triangles of a mesh in world coordinates, vertices moved `k` of the way to the coarser
/// level (the renderer's morph), with the caps of `faces`.
fn triangles(w: &WorldMesh, faces: &[usize], k: f64, out: &mut Vec<[DVec3; 3]>) {
    let at = |i: u32| {
        let i = i as usize;
        w.v.world[i] + w.v.normals[i].as_dvec3() * f64::from(w.v.morph[i]) * k
    };
    let caps = faces.iter().map(|&f| w.cap_faces[f].clone());
    let ranges = std::iter::once(0..w.surface_indices).chain(caps);
    for r in ranges {
        out.extend(w.indices[r].as_chunks::<3>().0.iter().map(|t| t.map(at)));
    }
}

/// Whether the segment from `a` to `b` meets any of the triangles (Möller & Trumbore).
fn blocked(tris: &[[DVec3; 3]], a: DVec3, b: DVec3) -> bool {
    let (lo, hi) = (a.min(b), a.max(b));
    let d = b - a;
    tris.iter().any(|&[p, q, r]| {
        if p.min(q).min(r).cmpgt(hi).any() || p.max(q).max(r).cmplt(lo).any() {
            return false;
        }
        let (e1, e2) = (q - p, r - p);
        let h = d.cross(e2);
        let det = e1.dot(h);
        if det.abs() < 1e-18 {
            return false;
        }
        let s = a - p;
        let u = s.dot(h) / det;
        let qv = s.cross(e1);
        let v = d.dot(qv) / det;
        let t = e2.dot(qv) / det;
        (0.0..=1.0).contains(&u) && v >= 0.0 && u + v <= 1.0 && (0.0..=1.0).contains(&t)
    })
}

/// A coarse chunk meets four finer ones across a face, the finer ones moved anywhere between
/// their own surface and the coarser one: rays from well above the ground to deep inside it,
/// steep and grazing, across and along the border, all meet the surface or a cap.
#[test]
fn between_levels_no_ray_reaches_the_rock_unseen() {
    let t = terrain();
    let (mut rays, mut level_faces, mut uncapped_misses) = (0, 0, 0);
    for p in spots(3) {
        for fine_level in [1u8, 3] {
            let coarse = ChunkKey::containing(p, fine_level + 1);
            let Some(wc) = world(coarse) else { continue };
            let c = coarse.cell_size();
            for face in 0..6 {
                let (axis, upper) = (face / 2, face % 2 == 1);
                let mut step = IVec3::ZERO;
                step[axis] = if upper { 1 } else { -1 };
                let across = ChunkKey {
                    pos: coarse.pos + step,
                    ..coarse
                };
                let fines: Vec<WorldMesh> = across
                    .children()
                    .into_iter()
                    .filter(|f| (f.pos[axis] % 2 == 0) == upper)
                    .filter_map(world)
                    .collect();
                // Border vertices of the coarse chunk on this face: where the ground meets it.
                let border: Vec<DVec3> = (0..wc.surface_vertices)
                    .filter(|&i| wc.v.planes[i] & (1 << face) != 0)
                    .map(|i| wc.v.world[i])
                    .collect();
                if fines.is_empty() || border.len() < 8 {
                    continue;
                }
                let up_axis = p.normalize()[axis].abs() > creeer_math::det::cos(15f64.to_radians());
                level_faces += usize::from(up_axis);
                // Inside both boxes, two coarse cells from every other face.
                let (lo, hi) = (
                    coarse.origin().min(across.origin()),
                    coarse.origin().max(across.origin()) + DVec3::splat(coarse.size()),
                );
                let inside = |q: DVec3| {
                    (0..3).all(|a| a == axis || (q[a] > lo[a] + 2.0 * c && q[a] < hi[a] - 2.0 * c))
                };
                for k in [0.0, 0.5, 1.0] {
                    let mut tris = Vec::new();
                    triangles(&wc, &[face], 0.0, &mut tris);
                    for wf in &fines {
                        triangles(wf, &[face ^ 1], k, &mut tris);
                    }
                    let mut bare = Vec::new();
                    triangles(&wc, &[], 0.0, &mut bare);
                    for wf in &fines {
                        triangles(wf, &[], k, &mut bare);
                    }
                    for (n, &s) in border.iter().enumerate().step_by(3) {
                        let up = s.normalize();
                        // A direction along the border (in the face, level).
                        let mut normal = DVec3::ZERO;
                        normal[axis] = 1.0;
                        let along = normal.cross(up).normalize_or_zero();
                        let mixed = |i: usize| {
                            f64::from(u32::try_from((n * 7 + i * 13) % 17).expect("small")) / 8.0
                                - 1.0
                        };
                        for i in 0..6 {
                            // Steep and grazing (down 6 cells over 0..8 sideways), across the
                            // face and along the border, through points near the border but
                            // not on its vertices (a ray exactly through a shared vertex is a
                            // tie this test cannot judge; the rasterizer's rules do).
                            let side = (along * mixed(i) * 4.0 + normal * mixed(i + 3) * 2.0) * c;
                            let near = s
                                + along * (0.5 * mixed(i + 1) + 0.137) * c
                                + normal * (0.25 * mixed(i + 2) + 0.071) * c;
                            let a = near + up * 3.0 * c + side;
                            let b = near - up * 3.0 * c - side;
                            if !inside(a)
                                || !inside(b)
                                || t.density(a) < 2.0 * c
                                || t.density(b) > -2.0 * c
                            {
                                continue;
                            }
                            rays += 1;
                            assert!(
                                blocked(&tris, a, b),
                                "ray {a} → {b} slips through ({coarse:?} face {face}, morph {k})"
                            );
                            uncapped_misses += usize::from(!blocked(&bare, a, b));
                        }
                    }
                }
            }
        }
    }
    println!(
        "{rays} rays, {level_faces} faces near level, {uncapped_misses} would miss without caps"
    );
    assert!(rays > 1000, "only {rays} rays");
    assert!(
        level_faces >= 2,
        "only {level_faces} faces lying near level"
    );
    // The rays do find the gaps between levels that the caps close.
    assert!(uncapped_misses > 0, "no ray tested a gap");
}
