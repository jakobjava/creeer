//! Surface Nets meshing of a chunk (Gibson 1998): one vertex per cell the surface passes
//! through, at the mean of the crossings on the cell's edges, and one quad per sample edge
//! with a sign change, joining the four cells around that edge. The result is smooth and
//! blockless, and cheaper than Dual Contouring (no QEF solve); ADR-0006.
//!
//! **Ownership:** a chunk makes the quads of the sample edges that start at its samples
//! 0..32 (`chunk.rs`), so every edge in space belongs to exactly one chunk, and neighbours of
//! one level share the vertices of the cells between them: no gaps, no overlaps.
//!
//! **Skirts:** where a chunk meets a neighbour of a different level, the two meshes sample
//! the surface at different resolutions and leave cracks of up to a coarse cell. Every
//! open border of a chunk's mesh therefore gets a skirt, a strip folded 3 cells down
//! into the rock along the vertex normals, which fills such cracks. Skirts are two-sided: a
//! crack is seen from either chunk's side. Next to a neighbour of the same level the skirt
//! lies inside the ground and is never seen.

use crate::chunk::{ChunkSamples, SAMPLES};
use creeer_math::glam::{DVec3, Vec3};
use std::collections::{HashMap, HashSet};

/// Skirt depth in cells of the chunk.
const SKIRT_CELLS: f32 = 3.0;

/// A chunk's triangle mesh, in coordinates relative to the chunk's origin.
#[derive(Clone, Debug, Default)]
pub struct TerrainMesh {
    /// Vertex positions (m, relative to the chunk origin).
    pub positions: Vec<Vec3>,
    /// Unit normals, pointing into the air.
    pub normals: Vec<Vec3>,
    /// Material of each vertex.
    pub materials: Vec<u16>,
    /// Triangle list, counter-clockwise seen from the air.
    pub indices: Vec<u32>,
    /// Indices before this belong to the surface, after it to skirts.
    pub surface_indices: usize,
    /// Vertices before this belong to the surface, after it to skirts.
    pub surface_vertices: usize,
    /// For each skirt vertex (from `surface_vertices` on), the surface vertex it hangs from.
    pub skirt_sources: Vec<u32>,
    /// How open each vertex is to the sky (0..1; empty until computed, `occlusion.rs`).
    pub openness: Vec<f32>,
    /// Index ranges of the skirts hanging from each face of the chunk (−X, +X, −Y, +Y, −Z,
    /// +Z). A skirt is only needed where the neighbour across that face has another level.
    pub skirt_faces: [std::ops::Range<usize>; 6],
}

impl TerrainMesh {
    /// Number of triangles, skirts included.
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// True if there is nothing to draw.
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }
}

const CORNERS: [[usize; 3]; 8] = [
    [0, 0, 0],
    [1, 0, 0],
    [0, 1, 0],
    [1, 1, 0],
    [0, 0, 1],
    [1, 0, 1],
    [0, 1, 1],
    [1, 1, 1],
];
/// Cell edges as pairs of corner indices.
const EDGES: [(usize, usize); 12] = [
    (0, 1),
    (2, 3),
    (4, 5),
    (6, 7),
    (0, 2),
    (1, 3),
    (4, 6),
    (5, 7),
    (0, 4),
    (1, 5),
    (2, 6),
    (3, 7),
];
/// Cells per axis with a vertex (cells −1..=31).
const CELLS: usize = SAMPLES - 1;

fn f(i: usize) -> f32 {
    #[allow(clippy::cast_precision_loss)] // indices up to 34
    let x = i as f32;
    x
}

/// Meshes a chunk; `material` gives the material at a world position with the surface
/// normal there (called once per vertex).
pub fn mesh(samples: &ChunkSamples, mut material: impl FnMut(DVec3, Vec3) -> u16) -> TerrainMesh {
    let mut mesh = TerrainMesh::default();
    if !samples.has_surface() {
        return mesh;
    }
    #[allow(clippy::cast_possible_truncation)] // cell sizes are powers of two
    let cell_size = samples.key.cell_size() as f32;
    let origin = samples.key.origin();
    let mut cell_vertex = vec![u32::MAX; CELLS * CELLS * CELLS];

    // 1. One vertex per cell that the surface crosses.
    for z in 0..CELLS {
        for y in 0..CELLS {
            for x in 0..CELLS {
                let d: [f32; 8] = std::array::from_fn(|c| {
                    let [i, j, k] = CORNERS[c];
                    samples.at(x + i, y + j, z + k)
                });
                let rock = d.iter().filter(|&&v| v < 0.0).count();
                if rock == 0 || rock == 8 {
                    continue;
                }
                let corner = |c: usize| {
                    let [i, j, k] = CORNERS[c];
                    Vec3::new(f(i), f(j), f(k))
                };
                let (mut sum, mut count) = (Vec3::ZERO, 0.0f32);
                for &(a, b) in &EDGES {
                    if (d[a] < 0.0) != (d[b] < 0.0) {
                        sum += corner(a).lerp(corner(b), d[a] / (d[a] - d[b]));
                        count += 1.0;
                    }
                }
                // The density rises into the air, so its gradient is the outward normal.
                let gradient = Vec3::new(
                    (d[1] - d[0]) + (d[3] - d[2]) + (d[5] - d[4]) + (d[7] - d[6]),
                    (d[2] - d[0]) + (d[3] - d[1]) + (d[6] - d[4]) + (d[7] - d[5]),
                    (d[4] - d[0]) + (d[5] - d[1]) + (d[6] - d[2]) + (d[7] - d[3]),
                );
                // Stored cell x is cell x − 1 of the chunk.
                let local = (Vec3::new(f(x), f(y), f(z)) - Vec3::ONE + sum / count) * cell_size;
                cell_vertex[x + CELLS * (y + CELLS * z)] =
                    u32::try_from(mesh.positions.len()).expect("vertex count");
                let normal = gradient.normalize_or(Vec3::Y);
                mesh.positions.push(local);
                mesh.normals.push(normal);
                mesh.materials
                    .push(material(origin + local.as_dvec3(), normal));
            }
        }
    }

    // 2. One quad per owned sample edge with a sign change. Edge along axis `a` from sample
    // q (stored indices 1..=32 in every axis, i.e. chunk samples 0..32); the four cells
    // around it are q−1 and q in the two other axes (b, c), with (a, b, c) cyclic.
    let vertex = |c: [usize; 3]| cell_vertex[c[0] + CELLS * (c[1] + CELLS * c[2])];
    for z in 1..SAMPLES - 1 {
        for y in 1..SAMPLES - 1 {
            for x in 1..SAMPLES - 1 {
                let q = [x, y, z];
                let rock = samples.at(x, y, z) < 0.0;
                for a in 0..3 {
                    let mut end = q;
                    end[a] += 1;
                    if (samples.at(end[0], end[1], end[2]) < 0.0) == rock {
                        continue;
                    }
                    let (b, c) = ((a + 1) % 3, (a + 2) % 3);
                    let cell = |db: usize, dc: usize| {
                        let mut m = q;
                        m[b] = m[b] + db - 1;
                        m[c] = m[c] + dc - 1;
                        vertex(m)
                    };
                    // Counter-clockwise seen from +a (b × c = a).
                    let mut quad = [cell(0, 0), cell(1, 0), cell(1, 1), cell(0, 1)];
                    if !rock {
                        // Rock at the far end: the surface faces −a.
                        quad.reverse();
                    }
                    push_quad(&mut mesh, quad);
                }
            }
        }
    }
    mesh.surface_indices = mesh.indices.len();
    mesh.surface_vertices = mesh.positions.len();
    add_skirts(&mut mesh, cell_size * SKIRT_CELLS, cell_size * 32.0);
    mesh
}

/// Two triangles, split along the shorter diagonal (better-shaped triangles).
fn push_quad(mesh: &mut TerrainMesh, [a, b, c, d]: [u32; 4]) {
    let p = |i: u32| mesh.positions[i as usize];
    if p(a).distance_squared(p(c)) <= p(b).distance_squared(p(d)) {
        mesh.indices.extend_from_slice(&[a, b, c, a, c, d]);
    } else {
        mesh.indices.extend_from_slice(&[a, b, d, b, c, d]);
    }
}

/// The face of a chunk of edge `size` that a border edge from `a` to `b` lies on (index as in
/// [`TerrainMesh::skirt_faces`]): the open borders run through the outermost cells, beyond
/// the chunk's faces.
fn face_of(a: Vec3, b: Vec3, size: f32) -> usize {
    let last = size * (31.0 / 32.0); // the outermost cell starts here
    for axis in 0..3 {
        if a[axis] < 0.0 && b[axis] < 0.0 {
            return 2 * axis;
        }
        if a[axis] > last && b[axis] > last {
            return 2 * axis + 1;
        }
    }
    // Not on a face (cannot happen for a chunk's open borders): the nearest one.
    let m = (a + b) * 0.5;
    let distances = [m.x, size - m.x, m.y, size - m.y, m.z, size - m.z];
    (0..6)
        .min_by(|&i, &j| distances[i].total_cmp(&distances[j]))
        .unwrap_or(0)
}

/// Folds every open border of the surface down into the rock by `depth`, grouped by the
/// chunk face (of edge `size`) it hangs from.
fn add_skirts(mesh: &mut TerrainMesh, depth: f32, size: f32) {
    let mut directed: HashSet<(u32, u32)> = HashSet::new();
    for tri in mesh.indices.as_chunks::<3>().0 {
        for e in 0..3 {
            directed.insert((tri[e], tri[(e + 1) % 3]));
        }
    }
    let mut border: Vec<(usize, u32, u32)> = directed
        .iter()
        .filter(|&&(a, b)| !directed.contains(&(b, a)))
        .map(|&(a, b)| {
            let (pa, pb) = (mesh.positions[a as usize], mesh.positions[b as usize]);
            (face_of(pa, pb, size), a, b)
        })
        .collect();
    border.sort_unstable(); // by face, deterministic
    let mut lowered: HashMap<u32, u32> = HashMap::new();
    let mut lower = |mesh: &mut TerrainMesh, v: u32| {
        *lowered.entry(v).or_insert_with(|| {
            let i = v as usize;
            let (p, n, m) = (mesh.positions[i], mesh.normals[i], mesh.materials[i]);
            mesh.positions.push(p - n * depth);
            mesh.normals.push(n);
            mesh.materials.push(m);
            mesh.skirt_sources.push(v);
            u32::try_from(mesh.positions.len() - 1).expect("vertex count")
        })
    };
    let mut face_start = [mesh.indices.len(); 7];
    let mut current = 0;
    for (face, a, b) in border {
        while current < face {
            current += 1;
            face_start[current] = mesh.indices.len();
        }
        let (a2, b2) = (lower(mesh, a), lower(mesh, b));
        // The surface runs a → b; the skirt continues it over the edge, running b → a. It is
        // drawn from both sides (the reversed copy), since the crack it fills can be seen
        // from either chunk.
        mesh.indices.extend_from_slice(&[b, a, a2, b, a2, b2]);
        mesh.indices.extend_from_slice(&[a, b, a2, a2, b, b2]);
    }
    for start in &mut face_start[current + 1..] {
        *start = mesh.indices.len();
    }
    mesh.skirt_faces = std::array::from_fn(|f| face_start[f]..face_start[f + 1]);
}
