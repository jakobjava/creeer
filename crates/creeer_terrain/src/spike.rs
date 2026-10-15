//! F0 spike (b): smooth voxel terrain throughput.
//!
//! A chunk is a 32³-cell block of a signed density field (positive = air, negative = rock).
//! The surface is where the density crosses zero. Surface Nets places one vertex per cell
//! that the surface passes through, at the average of the edge crossings, and connects
//! neighbouring cells with quads. The result is smooth and blockless.
//!
//! The spike measures generation (density sampling) and meshing separately, for chunks on
//! the surface of a 500 km planet. Seams between chunks and LOD are not handled here (F3).

use crate::noise::Perlin;
use creeer_math::glam::{DVec3, Vec3};

/// Cells per chunk edge.
pub const CHUNK_CELLS: usize = 32;
/// Samples per chunk edge (one more than cells).
const SAMPLES: usize = CHUNK_CELLS + 1;

/// Density samples of one chunk, `SAMPLES³` values, x fastest.
#[derive(Clone, Debug)]
pub struct DensityChunk {
    /// World position of sample (0, 0, 0) (m).
    pub origin: DVec3,
    /// Distance between samples (m).
    pub voxel_size: f64,
    values: Vec<f32>,
}

/// Triangle mesh in chunk-local coordinates (f32 is plenty within 32 m).
#[derive(Clone, Debug, Default)]
pub struct ChunkMesh {
    /// Vertex positions relative to the chunk origin (m).
    pub positions: Vec<Vec3>,
    /// Unit normals pointing into the air.
    pub normals: Vec<Vec3>,
    /// Triangle list.
    pub indices: Vec<u32>,
}

impl ChunkMesh {
    /// Number of triangles.
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

/// A planet: a sphere roughened by fractal noise.
#[derive(Clone, Debug)]
pub struct PlanetDensity {
    /// Planet radius (m).
    pub radius: f64,
    /// Height of the noise relief (m).
    pub relief: f64,
    /// Horizontal scale of the largest noise features (m).
    pub feature_size: f64,
    noise: Perlin,
}

impl PlanetDensity {
    /// The spike's reference planet: 500 km radius, ±60 m relief at 400 m scale, 6 octaves.
    pub fn reference(seed: u64) -> Self {
        Self {
            radius: 500_000.0,
            relief: 60.0,
            feature_size: 400.0,
            noise: Perlin::new(seed),
        }
    }

    /// Signed density at `p` (m): distance above the sphere minus the noise relief.
    pub fn density(&self, p: DVec3) -> f64 {
        let q = p / self.feature_size;
        p.length() - self.radius - self.relief * self.noise.fbm(q.x, q.y, q.z, 6)
    }

    /// Samples the density field of the chunk whose sample (0,0,0) is at `origin`.
    pub fn sample_chunk(&self, origin: DVec3, voxel_size: f64) -> DensityChunk {
        let mut values = Vec::with_capacity(SAMPLES * SAMPLES * SAMPLES);
        for z in 0..SAMPLES {
            for y in 0..SAMPLES {
                for x in 0..SAMPLES {
                    let offset = DVec3::new(index_f64(x), index_f64(y), index_f64(z)) * voxel_size;
                    #[allow(clippy::cast_possible_truncation)] // density near the surface is small
                    values.push(self.density(origin + offset) as f32);
                }
            }
        }
        DensityChunk {
            origin,
            voxel_size,
            values,
        }
    }
}

fn index_f64(i: usize) -> f64 {
    f64::from(u32::try_from(i).expect("small index"))
}

fn index_f32(i: usize) -> f32 {
    #[allow(clippy::cast_precision_loss)] // indices up to 33
    let f = i as f32;
    f
}

impl DensityChunk {
    /// A chunk from an arbitrary density function (used by tests).
    pub fn from_fn(origin: DVec3, voxel_size: f64, f: impl Fn(DVec3) -> f64) -> Self {
        let mut values = Vec::with_capacity(SAMPLES * SAMPLES * SAMPLES);
        for z in 0..SAMPLES {
            for y in 0..SAMPLES {
                for x in 0..SAMPLES {
                    let p =
                        origin + DVec3::new(index_f64(x), index_f64(y), index_f64(z)) * voxel_size;
                    #[allow(clippy::cast_possible_truncation)]
                    values.push(f(p) as f32);
                }
            }
        }
        Self {
            origin,
            voxel_size,
            values,
        }
    }

    fn at(&self, x: usize, y: usize, z: usize) -> f32 {
        self.values[x + SAMPLES * (y + SAMPLES * z)]
    }

    /// True if the surface passes through this chunk at all.
    pub fn has_surface(&self) -> bool {
        let first = self.values[0] < 0.0;
        self.values.iter().any(|&v| (v < 0.0) != first)
    }

    /// Builds the Surface Nets mesh.
    pub fn mesh(&self) -> ChunkMesh {
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
        let n = CHUNK_CELLS;
        #[allow(clippy::cast_possible_truncation)]
        let scale = self.voxel_size as f32;
        let mut mesh = ChunkMesh::default();
        let mut cell_vertex = vec![u32::MAX; n * n * n];

        // 1. One vertex per cell that the surface crosses.
        for z in 0..n {
            for y in 0..n {
                for x in 0..n {
                    let d: [f32; 8] = std::array::from_fn(|i| {
                        self.at(x + CORNERS[i][0], y + CORNERS[i][1], z + CORNERS[i][2])
                    });
                    let inside = d.iter().filter(|&&v| v < 0.0).count();
                    if inside == 0 || inside == 8 {
                        continue;
                    }
                    let corner = |i: usize| {
                        Vec3::new(
                            index_f32(CORNERS[i][0]),
                            index_f32(CORNERS[i][1]),
                            index_f32(CORNERS[i][2]),
                        )
                    };
                    let (mut sum, mut count) = (Vec3::ZERO, 0.0f32);
                    for &(a, b) in &EDGES {
                        if (d[a] < 0.0) != (d[b] < 0.0) {
                            let t = d[a] / (d[a] - d[b]);
                            sum += corner(a).lerp(corner(b), t);
                            count += 1.0;
                        }
                    }
                    let local = sum / count;
                    // Density gradient from the cell's corners points into the air.
                    let gradient = Vec3::new(
                        (d[1] - d[0]) + (d[3] - d[2]) + (d[5] - d[4]) + (d[7] - d[6]),
                        (d[2] - d[0]) + (d[3] - d[1]) + (d[6] - d[4]) + (d[7] - d[5]),
                        (d[4] - d[0]) + (d[5] - d[1]) + (d[6] - d[2]) + (d[7] - d[3]),
                    );
                    cell_vertex[x + n * (y + n * z)] =
                        u32::try_from(mesh.positions.len()).expect("vertex count");
                    let cell = Vec3::new(index_f32(x), index_f32(y), index_f32(z));
                    mesh.positions.push((cell + local) * scale);
                    mesh.normals.push(gradient.normalize_or_zero());
                }
            }
        }

        // 2. One quad per sample-grid edge with a sign change, joining the 4 cells around it.
        let vertex = |x: usize, y: usize, z: usize| cell_vertex[x + n * (y + n * z)];
        for z in 0..n {
            for y in 0..n {
                for x in 0..n {
                    let inside = self.at(x, y, z) < 0.0;
                    // Edges along +x, +y, +z from sample (x, y, z); the 4 cells sharing the
                    // edge need the other two coordinates ≥ 1 (the guard skips the wrapped
                    // indices computed for coordinate 0).
                    let axes = [
                        (
                            x + 1,
                            y,
                            z,
                            y >= 1 && z >= 1,
                            [
                                (x, y.wrapping_sub(1), z.wrapping_sub(1)),
                                (x, y, z.wrapping_sub(1)),
                                (x, y.wrapping_sub(1), z),
                                (x, y, z),
                            ],
                        ),
                        (
                            x,
                            y + 1,
                            z,
                            x >= 1 && z >= 1,
                            [
                                (x.wrapping_sub(1), y, z.wrapping_sub(1)),
                                (x.wrapping_sub(1), y, z),
                                (x, y, z.wrapping_sub(1)),
                                (x, y, z),
                            ],
                        ),
                        (
                            x,
                            y,
                            z + 1,
                            x >= 1 && y >= 1,
                            [
                                (x.wrapping_sub(1), y.wrapping_sub(1), z),
                                (x, y.wrapping_sub(1), z),
                                (x.wrapping_sub(1), y, z),
                                (x, y, z),
                            ],
                        ),
                    ];
                    for (ex, ey, ez, has_cells, cells) in axes {
                        if !has_cells || (self.at(ex, ey, ez) < 0.0) == inside {
                            continue;
                        }
                        let [a, b, c, d] = cells.map(|(cx, cy, cz)| vertex(cx, cy, cz));
                        if [a, b, c, d].contains(&u32::MAX) {
                            continue;
                        }
                        // Wind so triangles face the air side.
                        if inside {
                            mesh.indices.extend_from_slice(&[a, b, d, a, d, c]);
                        } else {
                            mesh.indices.extend_from_slice(&[a, c, d, a, d, b]);
                        }
                    }
                }
            }
        }
        mesh
    }
}

/// Origins of `count` chunks that intersect the reference planet's surface, spread around
/// the equator so the noise differs between them.
pub fn surface_chunk_origins(planet: &PlanetDensity, count: usize, voxel_size: f64) -> Vec<DVec3> {
    let size = index_f64(CHUNK_CELLS) * voxel_size;
    (0..count)
        .map(|i| {
            #[allow(clippy::cast_precision_loss)]
            let angle = i as f64 * 0.001;
            let (sin, cos) = creeer_math::det::sin_cos(angle);
            let surface = DVec3::new(cos, 0.0, sin) * planet.radius;
            // Centre the chunk on the surface point.
            surface - DVec3::splat(size / 2.0)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sphere_chunk(radius: f64) -> DensityChunk {
        let center = DVec3::splat(16.0);
        DensityChunk::from_fn(DVec3::ZERO, 1.0, |p| (p - center).length() - radius)
    }

    #[test]
    fn sphere_vertices_lie_on_the_sphere() {
        let mesh = sphere_chunk(10.0).mesh();
        assert!(
            mesh.triangle_count() > 500,
            "{} triangles",
            mesh.triangle_count()
        );
        let center = Vec3::splat(16.0);
        for p in &mesh.positions {
            let r = (*p - center).length();
            assert!((r - 10.0).abs() < 0.1, "vertex at radius {r}");
        }
    }

    #[test]
    fn normals_point_outwards_and_triangles_face_the_air() {
        let mesh = sphere_chunk(10.0).mesh();
        let center = Vec3::splat(16.0);
        for (p, n) in mesh.positions.iter().zip(&mesh.normals) {
            assert!(n.dot((*p - center).normalize()) > 0.9);
        }
        for tri in mesh.indices.as_chunks::<3>().0 {
            let [a, b, c] = [0, 1, 2].map(|i| mesh.positions[tri[i] as usize]);
            let face_normal = (b - a).cross(c - a);
            let outward = (a + b + c) / 3.0 - center;
            assert!(face_normal.dot(outward) > 0.0, "triangle faces inwards");
        }
    }

    #[test]
    fn closed_sphere_mesh_has_euler_characteristic_two() {
        // V − E + F = 2 for a closed surface of genus 0: checks there are no holes.
        let mesh = sphere_chunk(10.0).mesh();
        let mut edges = std::collections::HashSet::new();
        for tri in mesh.indices.as_chunks::<3>().0 {
            for (a, b) in [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
                edges.insert((a.min(b), a.max(b)));
            }
        }
        let euler = i64::try_from(mesh.positions.len()).unwrap()
            - i64::try_from(edges.len()).unwrap()
            + i64::try_from(mesh.triangle_count()).unwrap();
        assert_eq!(euler, 2);
    }

    #[test]
    fn empty_and_solid_chunks_produce_no_mesh() {
        let air = DensityChunk::from_fn(DVec3::ZERO, 1.0, |_| 1.0);
        let rock = DensityChunk::from_fn(DVec3::ZERO, 1.0, |_| -1.0);
        assert!(!air.has_surface() && !rock.has_surface());
        assert_eq!(air.mesh().triangle_count(), 0);
        assert_eq!(rock.mesh().triangle_count(), 0);
    }

    #[test]
    fn planet_surface_chunks_contain_terrain() {
        let planet = PlanetDensity::reference(1);
        for origin in surface_chunk_origins(&planet, 4, 1.0) {
            let chunk = planet.sample_chunk(origin, 1.0);
            assert!(chunk.has_surface());
            assert!(chunk.mesh().triangle_count() > 100);
        }
    }
}
