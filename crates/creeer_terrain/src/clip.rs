//! Clipping a chunk's surface at the chunk's box (Sutherland & Hodgman 1974, one face plane
//! at a time), so that the mesh is exactly the surface inside the box.
//!
//! Neighbours of one level clip the same triangles (`surface_nets.rs`) at their common face,
//! each keeping its side. Their borders must be the same points, bit for bit: a cut point is
//! computed from the segment's endpoints ordered along the plane's axis (whichever side
//! keeps it), and the planes are taken in a fixed order (−X, +X, −Y, +Y, −Z, +Z), which two
//! chunks sharing a face also share for every plane a small triangle can meet.

use crate::levels::Refined;
use crate::surface_nets::{Tri, Vertices, WorldMesh};
use creeer_math::glam::DVec3;
use std::collections::HashMap;

/// A piece of the clipped surface's border, lying on a face of the box.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Border {
    /// The face (−X, +X, −Y, +Y, −Z, +Z).
    pub face: usize,
    /// The border runs from vertex `a` to `b`, in the order of the triangle it bounds.
    pub a: u32,
    /// See `a`.
    pub b: u32,
    /// The rock end of the sample edge whose quad the triangle came from, if that edge lies
    /// on a face (stored sample indices).
    pub rock: Option<[usize; 3]>,
}

/// The clipped surface.
pub(crate) struct Clipped {
    /// Triangles inside the box.
    pub indices: Vec<u32>,
    /// Its border on the faces of the box.
    pub borders: Vec<Border>,
    /// Cut points made: (lower vertex, higher vertex, face) → vertex.
    cuts: HashMap<(u32, u32, usize), u32>,
}

impl Clipped {
    /// The vertex where the segment between vertices `a` and `b` crosses the plane of
    /// `face`, if clipping made one.
    pub fn cut(&self, a: u32, b: u32, face: usize) -> Option<u32> {
        self.cuts.get(&(a.min(b), a.max(b), face)).copied()
    }
}

/// Clips `tris` at the box from `lo` to `hi` (world); cut points become new vertices.
pub(crate) fn clip(v: &mut Vertices, tris: &[Tri], lo: DVec3, hi: DVec3) -> Clipped {
    let mut out = Clipped {
        indices: Vec::with_capacity(tris.len() * 3),
        borders: Vec::new(),
        cuts: HashMap::new(),
    };
    let plane = |face: usize| {
        let axis = face / 2;
        (
            axis,
            if face.is_multiple_of(2) {
                lo[axis]
            } else {
                hi[axis]
            },
        )
    };
    let outside = |v: &Vertices, i: u32, face: usize| {
        let i = i as usize;
        if v.planes[i] & (1 << face) != 0 {
            return false;
        }
        let (axis, value) = plane(face);
        if face.is_multiple_of(2) {
            v.world[i][axis] < value
        } else {
            v.world[i][axis] > value
        }
    };
    let mut poly: Vec<u32> = Vec::with_capacity(9);
    let mut next: Vec<u32> = Vec::with_capacity(9);
    for tri in tris {
        let codes = tri
            .v
            .map(|i| (0..6).fold(0u8, |c, f| c | (u8::from(outside(v, i, f)) << f)));
        if codes[0] | codes[1] | codes[2] == 0 {
            out.indices.extend_from_slice(&tri.v);
            continue;
        }
        if codes[0] & codes[1] & codes[2] != 0 {
            continue;
        }
        poly.clear();
        poly.extend_from_slice(&tri.v);
        for face in 0..6 {
            if (codes[0] | codes[1] | codes[2]) & (1 << face) == 0 || poly.is_empty() {
                continue;
            }
            next.clear();
            for k in 0..poly.len() {
                let (a, b) = (poly[k], poly[(k + 1) % poly.len()]);
                let (a_in, b_in) = (!outside(v, a, face), !outside(v, b, face));
                if a_in {
                    next.push(a);
                }
                if a_in != b_in {
                    next.push(cut(v, &mut out.cuts, a, b, face, plane(face)));
                }
            }
            std::mem::swap(&mut poly, &mut next);
        }
        if poly.len() < 3 {
            continue;
        }
        for k in 1..poly.len() - 1 {
            out.indices
                .extend_from_slice(&[poly[0], poly[k], poly[k + 1]]);
        }
        for k in 0..poly.len() {
            let (a, b) = (poly[k], poly[(k + 1) % poly.len()]);
            let shared = v.planes[a as usize] & v.planes[b as usize];
            for face in (0..6).filter(|f| shared & (1 << f) != 0) {
                out.borders.push(Border {
                    face,
                    a,
                    b,
                    rock: tri.rock,
                });
            }
        }
    }
    out
}

/// The point where the segment from vertex `a` to `b` crosses the plane `axis` = `value`,
/// as a vertex with interpolated attributes (made once per segment and face).
fn cut(
    v: &mut Vertices,
    cuts: &mut HashMap<(u32, u32, usize), u32>,
    a: u32,
    b: u32,
    face: usize,
    (axis, value): (usize, f64),
) -> u32 {
    if let Some(&i) = cuts.get(&(a.min(b), a.max(b), face)) {
        return i;
    }
    // From the endpoint lower along the axis: the same arithmetic on either side of the face.
    let (p, q) = if v.world[a as usize][axis] < v.world[b as usize][axis] {
        (a as usize, b as usize)
    } else {
        (b as usize, a as usize)
    };
    let (wp, wq) = (v.world[p], v.world[q]);
    let t = (value - wp[axis]) / (wq[axis] - wp[axis]);
    let mut at = wp + (wq - wp) * t;
    at[axis] = value;
    #[allow(clippy::cast_possible_truncation)] // a fraction
    let s = t as f32;
    let lerp = |x: creeer_math::glam::Vec3, y: creeer_math::glam::Vec3| {
        (x * (1.0 - s) + y * s).normalize_or(x)
    };
    let refined = Refined {
        position: at,
        normal: lerp(v.normals[p], v.normals[q]),
        coarse_normal: lerp(v.coarse_normals[p], v.coarse_normals[q]),
        morph: v.morph[p] * (1.0 - s) + v.morph[q] * s,
        anchor: at,
    };
    let material = if t <= 0.5 {
        v.materials[p]
    } else {
        v.materials[q]
    };
    let planes = (1 << face) | (v.planes[p] & v.planes[q]);
    let i = v.push(at, &refined, material, planes);
    cuts.insert((a.min(b), a.max(b), face), i);
    i
}

/// Drops the vertices no triangle uses (those clipping cut away), keeping the order.
pub(crate) fn compact(w: &mut WorldMesh) {
    let n = w.v.world.len();
    let mut used = vec![false; n];
    for &i in &w.indices {
        used[i as usize] = true;
    }
    let mut new_index = vec![u32::MAX; n];
    let mut kept = 0u32;
    for (i, &u) in used.iter().enumerate() {
        if u {
            new_index[i] = kept;
            kept += 1;
        }
    }
    for i in &mut w.indices {
        *i = new_index[*i as usize];
    }
    w.surface_vertices = used[..w.surface_vertices].iter().filter(|&&u| u).count();
    let v = &mut w.v;
    keep(&mut v.world, &used);
    keep(&mut v.normals, &used);
    keep(&mut v.coarse_normals, &used);
    keep(&mut v.morph, &used);
    keep(&mut v.materials, &used);
    keep(&mut v.anchors, &used);
    keep(&mut v.planes, &used);
}

fn keep<T>(list: &mut Vec<T>, used: &[bool]) {
    let mut k = 0;
    list.retain(|_| {
        k += 1;
        used[k - 1]
    });
}
