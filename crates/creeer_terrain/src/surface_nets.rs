//! Surface Nets meshing of a chunk (Gibson 1998): one vertex per cell the surface passes
//! through, at the mean of the crossings on the cell's edges, and one quad per sample edge
//! with a sign change, joining the four cells around that edge. The result is smooth and
//! blockless, and cheaper than Dual Contouring (no QEF solve); ADR-0006.
//!
//! **Ownership:** a chunk's mesh is exactly the surface inside the chunk's box. The chunk
//! builds every quad that reaches into the box — those of its sample edges 0..32, and the
//! quads straddling its upper faces, which need the samples one cell beyond (`chunk.rs`) —
//! and clips them at the box (`clip.rs`). Two neighbours of one level build the quads
//! straddling their common face from the same world positions, in the same way, so both
//! borders are the same points: no gaps, no overlaps.
//!
//! **Caps:** where a chunk meets a neighbour of another level, the two levels see slightly
//! different surfaces. Both end exactly on the common face, so whatever lies between them
//! lies in that plane, and on one level's rock side of it. Each chunk covers the rock it
//! cuts through on its faces with a cap (`caps.rs`), drawn where the neighbour has another
//! level: together, the two meshes and their caps close the ground (ADR-0006 amendment).

use crate::caps;
use crate::chunk::{ChunkSamples, SAMPLES};
use crate::clip;
use crate::levels::Refined;
use creeer_math::glam::{DVec3, Vec3};

/// A chunk's triangle mesh, in coordinates relative to the chunk's origin.
#[derive(Clone, Debug, Default)]
pub struct TerrainMesh {
    /// Vertex positions (m, relative to the chunk origin).
    pub positions: Vec<Vec3>,
    /// Unit normals, pointing into the air.
    pub normals: Vec<Vec3>,
    /// Normals of the next coarser level of detail (`levels.rs`; the normal itself without).
    pub coarse_normals: Vec<Vec3>,
    /// Distance along the normal to the next coarser level's surface (m; 0 without).
    pub morph: Vec<f32>,
    /// Material of each vertex.
    pub materials: Vec<u16>,
    /// Triangle list, counter-clockwise seen from the air (caps: seen from outside the box).
    pub indices: Vec<u32>,
    /// Indices before this belong to the surface, after it to caps.
    pub surface_indices: usize,
    /// Vertices before this belong to the surface, after it to caps.
    pub surface_vertices: usize,
    /// For each cap vertex (from `surface_vertices` on), the point of the surface it stands
    /// for (m, relative to the chunk origin): its shading is taken there.
    pub anchors: Vec<Vec3>,
    /// How open each vertex is to the sky (0..1; empty until computed, `occlusion.rs`).
    pub openness: Vec<f32>,
    /// Index ranges of the caps on each face of the chunk (−X, +X, −Y, +Y, −Z, +Z). A cap is
    /// only needed where the neighbour across that face has another level.
    pub cap_faces: [std::ops::Range<usize>; 6],
}

impl TerrainMesh {
    /// Number of triangles, caps included.
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// True if there is nothing to draw.
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Where vertex `v`'s shading is taken: the vertex itself, or a cap vertex's anchor.
    pub fn anchor(&self, v: usize) -> Vec3 {
        if v < self.surface_vertices {
            self.positions[v]
        } else {
            self.anchors[v - self.surface_vertices]
        }
    }
}

/// Where a vertex the mesher makes stands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Site {
    /// On the surface, for the cell from `low` with edge `edge`: it may move within it.
    Cell {
        /// The cell's lowest corner (world, m).
        low: DVec3,
        /// Its edge (m).
        edge: f64,
    },
    /// A cap's corner: it stays where it is. `depth` estimates its distance below the
    /// surface from the samples (m; negative above).
    Cap {
        /// Estimated depth (m).
        depth: f64,
    },
}

/// What the mesher asks for each vertex it makes: the vertex at a world position, with a
/// normal estimated from the samples, at its site.
pub type VertexFn<'a> = dyn FnMut(DVec3, Vec3, Site) -> (Refined, u16) + 'a;

/// Vertices under construction, in world coordinates.
#[derive(Default)]
pub(crate) struct Vertices {
    pub world: Vec<DVec3>,
    pub normals: Vec<Vec3>,
    pub coarse_normals: Vec<Vec3>,
    pub morph: Vec<f32>,
    pub materials: Vec<u16>,
    pub anchors: Vec<DVec3>,
    /// Bit f set: the vertex lies on the plane of face f (made there by clipping).
    pub planes: Vec<u8>,
}

impl Vertices {
    pub fn push(&mut self, at: DVec3, r: &Refined, material: u16, planes: u8) -> u32 {
        self.world.push(at);
        self.normals.push(r.normal);
        self.coarse_normals.push(r.coarse_normal);
        self.morph.push(r.morph);
        self.materials.push(material);
        self.anchors.push(r.anchor);
        self.planes.push(planes);
        u32::try_from(self.world.len() - 1).expect("vertex count")
    }
}

/// A surface triangle before clipping, and the rock end of the sample edge whose quad made
/// it if that edge lies on a face of the chunk (stored sample indices).
pub(crate) struct Tri {
    pub v: [u32; 3],
    pub rock: Option<[usize; 3]>,
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
/// Cells per axis with a vertex (cells −1..=32).
pub(crate) const CELLS: usize = SAMPLES - 1;
/// Stored index of the samples on the upper faces (chunk sample 32).
const UPPER: usize = SAMPLES - 2;

fn f(i: usize) -> f32 {
    #[allow(clippy::cast_precision_loss)] // indices up to 35
    let x = i as f32;
    x
}

/// Meshes a chunk; `material` gives the material at a world position with the surface
/// normal there (called once per vertex).
pub fn mesh(samples: &ChunkSamples, mut material: impl FnMut(DVec3, Vec3) -> u16) -> TerrainMesh {
    mesh_refined(samples, &mut |p, normal, site| {
        // A cap vertex stands for the surface above it along its normal.
        let anchor = match site {
            Site::Cell { .. } => p,
            Site::Cap { depth } => p + normal.as_dvec3() * depth,
        };
        let refined = Refined {
            position: p,
            normal,
            coarse_normal: normal,
            morph: 0.0,
            anchor,
        };
        (refined, material(anchor, normal))
    })
}

/// Meshes a chunk; `vertex` makes each vertex (`VertexFn`).
pub fn mesh_refined(samples: &ChunkSamples, vertex: &mut VertexFn<'_>) -> TerrainMesh {
    mesh_world(samples, vertex).map_or_else(TerrainMesh::default, |w| w.local(samples.key.origin()))
}

/// A chunk's mesh in world coordinates, as built (tests compare neighbours exactly).
pub(crate) struct WorldMesh {
    pub v: Vertices,
    pub indices: Vec<u32>,
    pub surface_indices: usize,
    pub surface_vertices: usize,
    pub cap_faces: [std::ops::Range<usize>; 6],
}

impl WorldMesh {
    /// The mesh relative to `origin`.
    pub fn local(self, origin: DVec3) -> TerrainMesh {
        let v = self.v;
        #[allow(clippy::cast_possible_truncation)] // within a chunk of its origin: fits f32
        let local = |p: &DVec3| (*p - origin).as_vec3();
        TerrainMesh {
            positions: v.world.iter().map(local).collect(),
            normals: v.normals,
            coarse_normals: v.coarse_normals,
            morph: v.morph,
            materials: v.materials,
            indices: self.indices,
            surface_indices: self.surface_indices,
            surface_vertices: self.surface_vertices,
            anchors: v.anchors[self.surface_vertices..]
                .iter()
                .map(local)
                .collect(),
            openness: Vec::new(),
            cap_faces: self.cap_faces,
        }
    }
}

/// Meshes a chunk in world coordinates; `None` without surface.
pub(crate) fn mesh_world(samples: &ChunkSamples, vertex: &mut VertexFn<'_>) -> Option<WorldMesh> {
    if !samples.has_surface() {
        return None;
    }
    let key = samples.key;
    let (origin, size) = (key.origin(), key.size());
    let mut v = Vertices::default();
    let cell_vertex = make_vertices(samples, &mut v, vertex);
    let tris = make_quads(samples, &v, &cell_vertex);
    let mut clipped = clip::clip(&mut v, &tris, origin, origin + DVec3::splat(size));
    let surface_vertices = v.world.len();
    let mut indices = std::mem::take(&mut clipped.indices);
    let surface_indices = indices.len();
    let at = |c: [usize; 3]| cell_vertex[c[0] + CELLS * (c[1] + CELLS * c[2])];
    let cap_faces = caps::add_caps(samples, &mut v, &clipped, &at, vertex, &mut indices);
    let mut world = WorldMesh {
        v,
        indices,
        surface_indices,
        surface_vertices,
        cap_faces,
    };
    clip::compact(&mut world);
    Some(world)
}

/// 1. One vertex per cell that the surface crosses; returns each cell's vertex (or
///    `u32::MAX`), cell x stored at x + 1.
fn make_vertices(samples: &ChunkSamples, v: &mut Vertices, vertex: &mut VertexFn<'_>) -> Vec<u32> {
    let key = samples.key;
    let (cell, size) = (key.cell_size(), key.size());
    let mut cell_vertex = vec![u32::MAX; CELLS * CELLS * CELLS];
    for z in 0..CELLS {
        for y in 0..CELLS {
            for x in 0..CELLS {
                let d: [f32; 8] = std::array::from_fn(|c| {
                    let [i, j, k] = CORNERS[c];
                    samples.at(x + i, y + j, z + k)
                });
                let rock = d.iter().filter(|&&s| s < 0.0).count();
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
                // In world coordinates, exactly as every chunk sharing this cell computes
                // it: the cell's corner is exact, and so is a fraction times a power of two.
                let low = samples.position(x, y, z);
                let p = low + (sum / count).as_dvec3() * cell;
                let (refined, material) = vertex(
                    p,
                    gradient.normalize_or(Vec3::Y),
                    Site::Cell { low, edge: cell },
                );
                let at = off_the_faces(refined.position, low, size);
                cell_vertex[x + CELLS * (y + CELLS * z)] = v.push(at, &refined, material, 0);
            }
        }
    }
    cell_vertex
}

/// Moves a vertex lying exactly on a chunk face of its level just into its own cell (from
/// `low`), so every vertex is strictly on its cell's side of every face: the quads straddling
/// a face are then exactly those of the sample edges on it, and both neighbours (which see
/// the same cell) clip alike.
fn off_the_faces(mut p: DVec3, low: DVec3, size: f64) -> DVec3 {
    for a in 0..3 {
        if (p[a] / size).fract() == 0.0 {
            p[a] = if p[a] <= low[a] {
                p[a].next_up()
            } else {
                p[a].next_down()
            };
        }
    }
    p
}

/// 2. One quad per sample edge with a sign change whose quad reaches into the box: edges
///    along axis `a` from sample q (stored indices 1..=33 in every axis; along `a` only up to
///    32, edges further out lie beyond the upper face). The four cells around it are q−1 and q
///    in the two other axes (b, c), with (a, b, c) cyclic.
fn make_quads(samples: &ChunkSamples, v: &Vertices, cell_vertex: &[u32]) -> Vec<Tri> {
    let vertex = |c: [usize; 3]| cell_vertex[c[0] + CELLS * (c[1] + CELLS * c[2])];
    let on_face = |i: usize| i == 1 || i == UPPER;
    let mut tris = Vec::new();
    for z in 1..=UPPER {
        for y in 1..=UPPER {
            for x in 1..=UPPER {
                let q = [x, y, z];
                let rock = samples.at(x, y, z) < 0.0;
                for a in 0..3 {
                    if q[a] == UPPER {
                        continue;
                    }
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
                    let edge_rock =
                        (on_face(q[b]) || on_face(q[c])).then_some(if rock { q } else { end });
                    push_quad(&mut tris, &v.world, quad, edge_rock);
                }
            }
        }
    }
    tris
}

/// Two triangles, split along the shorter diagonal (better-shaped triangles), decided in
/// world coordinates so that neighbours split alike.
fn push_quad(
    tris: &mut Vec<Tri>,
    world: &[DVec3],
    [a, b, c, d]: [u32; 4],
    rock: Option<[usize; 3]>,
) {
    let p = |i: u32| world[i as usize];
    let (first, second) = if p(a).distance_squared(p(c)) <= p(b).distance_squared(p(d)) {
        ([a, b, c], [a, c, d])
    } else {
        ([a, b, d], [b, c, d])
    };
    tris.push(Tri { v: first, rock });
    tris.push(Tri { v: second, rock });
}
