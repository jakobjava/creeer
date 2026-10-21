//! The primitives' meshes (`primitives.rs`). Every vertex of a rounded primitive sits at its
//! corner of the unit shape; its place once rounded comes from its normal (`shaped`).

use super::{corner_index, corner_normals, corner_triangles, turn};
use crate::mesh::{MeshData, MeshVertex};
use creeer_math::glam::Vec3;

impl MeshData {
    fn push(&mut self, vertex: MeshVertex) -> u32 {
        let index = u32::try_from(self.vertices.len()).expect("mesh too large");
        self.vertices.push(vertex);
        index
    }

    /// Adds triangle `t`, wound to face the way its vertices' normals do once rounded (by a
    /// radius of 0.1 at scale 1).
    fn push_facing(&mut self, t: [u32; 3]) {
        let vertex = |i: u32| &self.vertices[i as usize];
        let at = |i: u32| {
            let p = Vec3::from(vertex(i).position);
            p + (Vec3::from(vertex(i).normal) - 2.0 * p) * 0.1
        };
        let outward: Vec3 = t.iter().map(|&i| Vec3::from(vertex(i).normal)).sum();
        let [a, b, c] = t.map(at);
        let t = if (b - a).cross(c - a).dot(outward) >= 0.0 {
            t
        } else {
            [t[0], t[2], t[1]]
        };
        self.indices.extend_from_slice(&t);
    }

    /// Adds quad `q` (its corners in order around) as two triangles.
    fn push_round_quad(&mut self, [a, b, c, d]: [u32; 4]) {
        self.push_facing([a, b, c]);
        self.push_facing([a, c, d]);
    }

    /// The unit box with its edges and corners rounded `segments` a quarter turn (1: a
    /// chamfer): six faces, twelve edge strips and eight corner patches, 8k² + 24k + 12
    /// triangles.
    pub fn rounded_box(segments: u32) -> Self {
        let k = segments.max(1);
        let normals = corner_normals(k);
        let patch = u32::try_from(normals.len()).expect("a small patch");
        // Corner c's signs: bits 0, 1 and 2 set for negative x, y and z.
        let signs = |c: u32| {
            let sign = |bit: u32| if c & bit == 0 { 1.0 } else { -1.0 };
            Vec3::new(sign(1), sign(2), sign(4))
        };
        let mut mesh = Self::default();
        for c in 0..8 {
            let s = signs(c);
            for &n in &normals {
                mesh.push(MeshVertex::bevelled(
                    (0.5 * s).to_array(),
                    (n * s).to_array(),
                ));
            }
        }
        let at = |c: u32, (i, j): (u32, u32)| c * patch + corner_index(k, i, j);
        for c in 0..8 {
            for t in corner_triangles(k) {
                mesh.push_facing(t.map(|i| c * patch + i));
            }
        }
        for axis in 0..3 {
            let bit = 1 << axis;
            // Edge strips: between two corners apart along `axis`, joining their arcs that
            // have no part of it.
            let arc = |m: u32| match axis {
                0 => (0, m),
                1 => (m, 0),
                _ => (m, k - m),
            };
            for c in (0..8).filter(|c| c & bit == 0) {
                for m in 0..k {
                    let (p, q) = (arc(m), arc(m + 1));
                    mesh.push_round_quad([at(c, p), at(c, q), at(c | bit, q), at(c | bit, p)]);
                }
            }
            // The two faces across `axis`, between their corners' vertices facing along it.
            let flat = match axis {
                0 => (k, 0),
                1 => (0, k),
                _ => (0, 0),
            };
            let (u, v) = (1 << ((axis + 1) % 3), 1 << ((axis + 2) % 3));
            for side in [0, bit] {
                mesh.push_round_quad([0, u, u | v, v].map(|o| at(side | o, flat)));
            }
        }
        mesh
    }

    /// The unit cylinder (radius 0.5 about Y, from −0.5 to 0.5) with `segments` around and its
    /// rims rounded `rim` segments a quarter turn; with none, sharp ([`MeshData::cylinder`]).
    /// 4·rim + 4 triangles a segment.
    pub fn rounded_cylinder(segments: u32, rim: u32) -> Self {
        if rim == 0 {
            return Self::cylinder(0.5, 0.5, segments);
        }
        let n = segments.max(3);
        let around = |i: u32| {
            let phi = turn(i % n, n);
            Vec3::new(phi.cos(), 0.0, -phi.sin())
        };
        let w = |m: u32| turn(m, 4 * rim).sin();
        let mut mesh = Self::default();
        // Each end: its rim, arc by arc from the cap (m = 0) to the side (m = rim), then its
        // centre.
        let mut ends = Vec::new();
        for up in [Vec3::Y, Vec3::NEG_Y] {
            let first = u32::try_from(mesh.vertices.len()).expect("a small mesh");
            for i in 0..n {
                let p = 0.5 * (around(i) + up);
                for m in 0..=rim {
                    let normal = around(i) * w(m) + up * w(rim - m);
                    mesh.push(MeshVertex::bevelled(p.to_array(), normal.to_array()));
                }
            }
            let centre = mesh.push(MeshVertex::bevelled((0.5 * up).to_array(), up.to_array()));
            ends.push((first, centre));
        }
        let ring = |first: u32, i: u32, m: u32| first + (i % n) * (rim + 1) + m;
        let (top, bottom) = (ends[0].0, ends[1].0);
        for i in 0..n {
            let side = |first, i| ring(first, i, rim);
            mesh.push_round_quad([
                side(top, i),
                side(top, i + 1),
                side(bottom, i + 1),
                side(bottom, i),
            ]);
            for &(first, centre) in &ends {
                for m in 0..rim {
                    let r = |i, m| ring(first, i, m);
                    mesh.push_round_quad([r(i, m), r(i + 1, m), r(i + 1, m + 1), r(i, m + 1)]);
                }
                mesh.push_facing([centre, ring(first, i, 0), ring(first, i + 1, 0)]);
            }
        }
        mesh
    }
}
