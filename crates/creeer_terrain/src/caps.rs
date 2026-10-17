//! Caps: the rock a chunk's box cuts through, on each of its faces.
//!
//! A chunk's mesh ends exactly on its faces (`clip.rs`); its solid — the rock inside the box —
//! is closed by the mesh and the caps together. Next to a neighbour of another level both
//! surfaces end on the common face but differ slightly, and wherever one level leaves the
//! plane open, the other has rock there: its cap closes the gap, however flat the ground
//! meets the face (skirts hanging down from the borders could not: ground nearly parallel
//! to a face left flat strips open, measured in F4).
//!
//! On a face, the samples lying on it form a grid of square cells, and the surface crosses
//! the face between the vertex of the cell inside the box and the one outside (each grid
//! cell has one of each): the border runs through those crossings ("nodes", one per grid
//! cell the surface crosses), with a kink where a quad's diagonal crosses the face. The rock
//! part of the face is covered by
//!
//! - a triangle from every border piece to the rock end of the grid edge it crosses;
//! - in each crossed grid cell, a triangle from the node to every edge with rock at both ends;
//! - grid cells all in rock: per row, a strip over each run of them, with a vertex wherever a
//!   cell beside the line is not all rock, so caps meet without T-junctions.

use crate::chunk::{ChunkSamples, SAMPLES};
use crate::clip::Clipped;
use crate::surface_nets::{Site, VertexFn, Vertices};
use creeer_math::glam::Vec3;
use std::collections::HashMap;
use std::ops::Range;

/// Grid corners per face edge (chunk samples 0..=32).
const N: usize = 33;

/// Adds the caps of all six faces to `indices`; returns each face's index range.
pub(crate) fn add_caps(
    samples: &ChunkSamples,
    v: &mut Vertices,
    clipped: &Clipped,
    cell_vertex: &dyn Fn([usize; 3]) -> u32,
    vertex: &mut VertexFn<'_>,
    indices: &mut Vec<u32>,
) -> [Range<usize>; 6] {
    // Corners on an edge of the box belong to two faces: one vertex serves both caps.
    let mut corners = HashMap::new();
    std::array::from_fn(|face| {
        let start = indices.len();
        Face::new(samples, face, &mut corners).cap(v, clipped, cell_vertex, vertex, indices);
        start..indices.len()
    })
}

/// One face of the box and the grid of samples on it.
struct Face<'a, 'b> {
    samples: &'a ChunkSamples,
    face: usize,
    /// Normal axis and the two in-plane axes (u, v, w cyclic: v × w = u).
    axes: [usize; 3],
    /// Stored sample index of the face's plane along u.
    plane: usize,
    /// Rock at grid corner (i, j) (i along v, j along w).
    rock: Vec<bool>,
    /// The cap vertex at each sample, once made (shared by all faces).
    corners: &'b mut HashMap<[usize; 3], u32>,
}

impl<'a, 'b> Face<'a, 'b> {
    fn new(
        samples: &'a ChunkSamples,
        face: usize,
        corners: &'b mut HashMap<[usize; 3], u32>,
    ) -> Self {
        let u = face / 2;
        let plane = if face.is_multiple_of(2) {
            1
        } else {
            SAMPLES - 2
        };
        let mut f = Self {
            samples,
            face,
            axes: [u, (u + 1) % 3, (u + 2) % 3],
            plane,
            rock: Vec::with_capacity(N * N),
            corners,
        };
        for j in 0..N {
            for i in 0..N {
                let s = f.stored(i, j);
                f.rock.push(samples.at(s[0], s[1], s[2]) < 0.0);
            }
        }
        f
    }

    /// Stored sample indices of grid corner (i, j).
    fn stored(&self, i: usize, j: usize) -> [usize; 3] {
        let [u, a, b] = self.axes;
        let mut s = [0; 3];
        s[u] = self.plane;
        s[a] = i + 1;
        s[b] = j + 1;
        s
    }

    fn is_rock(&self, i: usize, j: usize) -> bool {
        self.rock[i + N * j]
    }

    /// The cap vertex at grid corner (i, j).
    fn corner(&mut self, v: &mut Vertices, vertex: &mut VertexFn<'_>, i: usize, j: usize) -> u32 {
        let s = self.stored(i, j);
        if let Some(&made) = self.corners.get(&s) {
            return made;
        }
        let p = self.samples.position(s[0], s[1], s[2]);
        // Depth below the surface from the samples: density over its slope.
        let gradient = self.sample_gradient(s);
        let depth = -f64::from(self.samples.at(s[0], s[1], s[2]) / gradient.length().max(1e-6));
        let site = Site::Cap { depth };
        let (refined, material) = vertex(p, gradient.normalize_or(Vec3::Y), site);
        let made = v.push(p, &refined, material, 1 << self.face);
        self.corners.insert(s, made);
        made
    }

    /// The density gradient at a sample (per metre) from central differences of its
    /// neighbours (all on the grid: the faces lie one sample inside it).
    fn sample_gradient(&self, s: [usize; 3]) -> Vec3 {
        let at = |axis: usize, step: isize| {
            let mut k = s;
            k[axis] = k[axis].checked_add_signed(step).expect("inside the grid");
            self.samples.at(k[0], k[1], k[2])
        };
        #[allow(clippy::cast_possible_truncation)] // cell sizes are powers of two
        let across = 2.0 * self.samples.key.cell_size() as f32;
        Vec3::new(
            at(0, 1) - at(0, -1),
            at(1, 1) - at(1, -1),
            at(2, 1) - at(2, -1),
        ) / across
    }

    /// Appends a cap triangle, wound counter-clockwise seen from outside the box.
    fn emit(&self, v: &Vertices, indices: &mut Vec<u32>, [a, b, c]: [u32; 3]) {
        let [_, x, y] = self.axes;
        let p = |i: u32| {
            let w = v.world[i as usize];
            (w[x], w[y])
        };
        let ((ax, ay), (bx, by), (cx, cy)) = (p(a), p(b), p(c));
        let area = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax);
        // Seen from +u, (v, w) is counter-clockwise; a lower face is seen from −u.
        let outward = if self.face.is_multiple_of(2) {
            -area
        } else {
            area
        };
        // A degenerate one is kept too: it draws nothing, and keeps the cap closed.
        if outward < 0.0 {
            indices.extend_from_slice(&[a, c, b]);
        } else {
            indices.extend_from_slice(&[a, b, c]);
        }
    }

    fn cap(
        &mut self,
        v: &mut Vertices,
        clipped: &Clipped,
        cell_vertex: &dyn Fn([usize; 3]) -> u32,
        vertex: &mut VertexFn<'_>,
        indices: &mut Vec<u32>,
    ) {
        let [u, a, b] = self.axes;
        // Border pieces to the rock end of the grid edge they cross.
        let face = self.face;
        for border in clipped.borders.iter().filter(|e| e.face == face) {
            let corner = border
                .rock
                .filter(|r| r[u] == self.plane)
                .map(|r| (r[a] - 1, r[b] - 1))
                .or_else(|| self.nearest_rock(v, border.a, border.b));
            if let Some((i, j)) = corner {
                let r = self.corner(v, vertex, i, j);
                self.emit(v, indices, [border.a, border.b, r]);
            }
        }
        // Crossed grid cells: node to the edges in rock; cells all in rock: strips.
        let (inner, outer) = if self.face.is_multiple_of(2) {
            (1, 0)
        } else {
            (SAMPLES - 3, SAMPLES - 2)
        };
        let mut full = vec![false; (N - 1) * (N - 1)];
        for j in 0..N - 1 {
            for i in 0..N - 1 {
                let ring = [(i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1)];
                let rocks = ring.map(|(x, y)| self.is_rock(x, y));
                if rocks.iter().all(|&r| r) {
                    full[i + (N - 1) * j] = true;
                    continue;
                }
                if !rocks.iter().any(|&r| r) {
                    continue;
                }
                let mut cell = [0; 3];
                (cell[a], cell[b]) = (i + 1, j + 1);
                cell[u] = inner;
                let inside = cell_vertex(cell);
                cell[u] = outer;
                let beyond = cell_vertex(cell);
                let Some(node) = clipped.cut(inside, beyond, self.face) else {
                    let key = self.samples.key;
                    debug_assert!(
                        false,
                        "no border node: {key:?} face {} cell {cell:?}",
                        self.face
                    );
                    continue;
                };
                for k in 0..4 {
                    if rocks[k] && rocks[(k + 1) % 4] {
                        let (p, q) = (ring[k], ring[(k + 1) % 4]);
                        let p = self.corner(v, vertex, p.0, p.1);
                        let q = self.corner(v, vertex, q.0, q.1);
                        self.emit(v, indices, [node, p, q]);
                    }
                }
            }
        }
        self.strips(v, vertex, indices, &full);
    }

    /// Strips over the runs of grid cells all in rock, row by row.
    fn strips(
        &mut self,
        v: &mut Vertices,
        vertex: &mut VertexFn<'_>,
        indices: &mut Vec<u32>,
        full: &[bool],
    ) {
        let cells = N - 1;
        let is_full = |i: usize, j: usize| full[i + cells * j];
        // Corners on grid line y that some cell beside the line needs as a vertex, and every
        // corner on the face's rim, where the caps of the faces around meet theirs.
        let needed = |x: usize, y: usize| {
            if x == 0 || x == cells || y == 0 || y == cells {
                return true;
            }
            let rows = [y.checked_sub(1), (y < cells).then_some(y)];
            rows.into_iter()
                .flatten()
                .any(|row| !is_full(x - 1, row) || !is_full(x, row))
        };
        for j in 0..cells {
            let mut i = 0;
            while i < cells {
                if !is_full(i, j) {
                    i += 1;
                    continue;
                }
                let start = i;
                while i < cells && is_full(i, j) {
                    i += 1;
                }
                let bottom: Vec<usize> = (start..=i).filter(|&x| needed(x, j)).collect();
                let top: Vec<usize> = (start..=i).filter(|&x| needed(x, j + 1)).collect();
                let (mut bi, mut ti) = (0, 0);
                while bi + 1 < bottom.len() || ti + 1 < top.len() {
                    let advance_bottom = ti + 1 == top.len()
                        || (bi + 1 < bottom.len() && bottom[bi + 1] <= top[ti + 1]);
                    let tri = if advance_bottom {
                        bi += 1;
                        [(bottom[bi - 1], j), (bottom[bi], j), (top[ti], j + 1)]
                    } else {
                        ti += 1;
                        [(bottom[bi], j), (top[ti], j + 1), (top[ti - 1], j + 1)]
                    };
                    let tri = tri.map(|(x, y)| self.corner(v, vertex, x, y));
                    self.emit(v, indices, tri);
                }
            }
        }
    }

    /// For a border piece whose grid edge is unknown: the rock corner of the grid cell under
    /// its middle nearest to it.
    fn nearest_rock(&self, v: &Vertices, a: u32, b: u32) -> Option<(usize, usize)> {
        let [_, x, y] = self.axes;
        let mid = (v.world[a as usize] + v.world[b as usize]) * 0.5;
        let key = self.samples.key;
        let local = (mid - key.origin()) / key.cell_size();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped to the grid
        let cell = |c: f64| c.floor().clamp(0.0, (N - 2) as f64) as usize;
        let (i, j) = (cell(local[x]), cell(local[y]));
        let distance = |&(p, q): &(usize, usize)| {
            let index = |n: usize| f64::from(u32::try_from(n).expect("grid index"));
            let (a, b) = (local[x] - index(p), local[y] - index(q));
            (a * a + b * b).sqrt()
        };
        [(i, j), (i + 1, j), (i, j + 1), (i + 1, j + 1)]
            .into_iter()
            .filter(|&(p, q)| self.is_rock(p, q))
            .min_by(|a, b| distance(a).total_cmp(&distance(b)))
    }
}
