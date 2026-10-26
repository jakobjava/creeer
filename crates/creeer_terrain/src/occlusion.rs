//! How open each terrain vertex is to the sky (ambient occlusion), from the chunk's own
//! density samples.
//!
//! Light from the sky reaches a point only through the air around it: inside a tunnel almost
//! none does, at the foot of a cliff half of it. From every vertex, eight rays in a cone around
//! its normal march through the density grid (1, 2, 4 and 6 cells); a ray that enters rock is
//! blocked from there on. Openness is the cosine-weighted fraction of open rays (1 on open
//! ground). Rays leaving the chunk's grid count as open: occlusion is a local effect, and the
//! chunk sees one cell beyond its edges.

use crate::chunk::{ChunkSamples, SAMPLES};
use crate::surface_nets::TerrainMesh;
use creeer_math::glam::Vec3;

/// Distances along each ray (cells).
const STEPS: [f32; 4] = [1.0, 2.0, 4.0, 6.0];

impl ChunkSamples {
    /// Density by trilinear interpolation at `p` in sample coordinates (sample (i, j, k) at
    /// (i, j, k)); `None` outside the grid.
    pub fn interpolate(&self, p: Vec3) -> Option<f32> {
        let max = (SAMPLES - 1) as f32;
        if p.min_element() < 0.0 || p.max_element() >= max {
            return None;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // inside the grid
        let [i, j, k] = p.to_array().map(|x| x.floor() as usize);
        let f = p - Vec3::new(i as f32, j as f32, k as f32);
        let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
        let face = |dk: usize| {
            let row = |dj: usize| {
                lerp(
                    self.at(i, j + dj, k + dk),
                    self.at(i + 1, j + dj, k + dk),
                    f.x,
                )
            };
            lerp(row(0), row(1), f.y)
        };
        Some(lerp(face(0), face(1), f.z))
    }
}

/// Ray directions in a cone around +Z (45° and 70° from the axis, alternating), unit length.
fn cone() -> [Vec3; 8] {
    std::array::from_fn(|i| {
        let angle = (i as f32) * std::f32::consts::TAU / 8.0;
        let tilt = if i % 2 == 0 { 45f32 } else { 70f32 }.to_radians();
        Vec3::new(
            tilt.sin() * angle.cos(),
            tilt.sin() * angle.sin(),
            tilt.cos(),
        )
    })
}

/// Openness (0..1) of every vertex of `mesh`, built from `samples`.
pub fn openness(samples: &ChunkSamples, mesh: &TerrainMesh) -> Vec<f32> {
    #[allow(clippy::cast_possible_truncation)] // cell sizes are powers of two
    let cell = samples.key.cell_size() as f32;
    let rays = cone();
    let mut out = Vec::with_capacity(mesh.positions.len());
    for v in 0..mesh.positions.len() {
        let n = mesh.normals[v];
        // Sample coordinates: the grid starts one cell before the chunk origin.
        let start = mesh.anchor(v) / cell + Vec3::ONE;
        let (t, b) = n.any_orthonormal_pair();
        let (mut open, mut total) = (0.0f32, 0.0f32);
        for ray in rays {
            let dir = t * ray.x + b * ray.y + n * ray.z;
            let weight = ray.z; // cosine weighting
            total += weight;
            let blocked = STEPS.iter().any(|&s| {
                samples
                    .interpolate(start + dir * s)
                    .is_some_and(|d| d < 0.0)
            });
            if !blocked {
                open += weight;
            }
        }
        out.push(open / total);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::ChunkKey;
    use crate::surface_nets::mesh;
    use creeer_math::glam::{DVec3, IVec3};

    fn key() -> ChunkKey {
        ChunkKey {
            level: 3,
            pos: IVec3::ZERO,
        }
    }

    #[test]
    fn open_ground_is_fully_open() {
        let s = ChunkSamples::from_fn(key(), |p| p.y - 12.3);
        let m = mesh(&s, |_, _| 0);
        let o = openness(&s, &m);
        assert_eq!(o.len(), m.positions.len());
        assert!(o.iter().all(|&x| (x - 1.0).abs() < 1e-6));
    }

    #[test]
    fn a_tunnel_is_dark_inside() {
        // A horizontal tube of air, radius 2.5 m, through solid rock.
        let s = ChunkSamples::from_fn(key(), |p| {
            2.5 - ((p - DVec3::new(p.x, 16.0, 16.0)).length())
        });
        let m = mesh(&s, |_, _| 0);
        let o = openness(&s, &m);
        let mean: f32 = o[..m.surface_vertices].iter().sum::<f32>() / m.surface_vertices as f32;
        // Rays along the tube escape at the grid ends; the sky view of the terrain makes a
        // real tunnel dark (`Terrain::mesh_chunk`).
        assert!(mean < 0.5, "tunnel openness {mean}");
    }

    #[test]
    fn interpolation_reproduces_linear_fields() {
        let s = ChunkSamples::from_fn(key(), |p| 0.25 * p.x - 0.5 * p.y + p.z);
        let q = Vec3::new(7.3, 11.6, 20.05);
        // Sample (i, j, k) sits at world (i − 1, j − 1, k − 1) for a level-0 chunk at the origin.
        let world = q - Vec3::ONE;
        let expected = 0.25 * world.x - 0.5 * world.y + world.z;
        assert!((s.interpolate(q).expect("inside") - expected).abs() < 1e-4);
        assert!(s.interpolate(Vec3::splat(-0.5)).is_none());
    }
}
