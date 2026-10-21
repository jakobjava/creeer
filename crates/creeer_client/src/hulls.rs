//! The convex hulls of the physics sandbox's rock pile (rocks, a wedge, a hexagonal prism),
//! made once: the simulation collides with them and the renderer draws them, flat-shaded.

use creeer_math::det;
use creeer_math::glam::DVec3;
use creeer_physics::world::hull::Hull;
use creeer_render::{MeshData, MeshVertex};
use std::sync::{Arc, OnceLock};

/// How many hulls there are.
pub const HULLS: usize = 8;
/// The rocks among them (the first ones).
pub const ROCKS: usize = 6;
/// The wedge.
pub const WEDGE: usize = 6;
/// The hexagonal prism.
pub const PRISM: usize = 7;

/// The hulls, each centred on its centroid.
pub fn catalogue() -> &'static [Arc<Hull>; HULLS] {
    static MADE: OnceLock<[Arc<Hull>; HULLS]> = OnceLock::new();
    MADE.get_or_init(|| {
        std::array::from_fn(|k| {
            let points = match k {
                WEDGE => wedge(),
                PRISM => prism(),
                _ => rock(k),
            };
            Arc::new(
                Hull::new(&points)
                    .expect("the catalogue's points enclose a volume")
                    .0,
            )
        })
    })
}

/// Which hull of the catalogue `hull` is, if it is one.
pub fn index_of(hull: &Arc<Hull>) -> Option<usize> {
    catalogue().iter().position(|h| Arc::ptr_eq(h, hull))
}

/// A rock: twelve points on a flattened ball about 0.6 m across, scattered (deterministic).
fn rock(seed: usize) -> Vec<DVec3> {
    let mut state = 0x9e37_79b9_7f4a_7c15_u64.wrapping_mul(seed as u64 + 1);
    let mut next = move || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        #[allow(clippy::cast_precision_loss)] // 53 bits of it
        let unit = (state >> 11) as f64 / (1u64 << 53) as f64;
        unit
    };
    let shape = DVec3::new(0.35, 0.22, 0.28) * (0.8 + 0.4 * next());
    let mut points = Vec::new();
    while points.len() < 12 {
        let p = DVec3::new(2.0 * next() - 1.0, 2.0 * next() - 1.0, 2.0 * next() - 1.0);
        if p.length_squared() <= 1.0 && p.length_squared() > 0.05 {
            points.push(p.normalize() * shape * (0.85 + 0.15 * next()));
        }
    }
    points
}

/// A wedge 1 m long, 0.5 m high at its back and 0.6 m wide.
fn wedge() -> Vec<DVec3> {
    let mut points = Vec::new();
    for z in [-0.3, 0.3] {
        points.extend([
            DVec3::new(-0.5, 0.0, z),
            DVec3::new(0.5, 0.0, z),
            DVec3::new(-0.5, 0.5, z),
        ]);
    }
    points
}

/// A hexagonal prism 0.6 m across its corners and 0.8 m long, along z.
fn prism() -> Vec<DVec3> {
    let mut points = Vec::new();
    for k in 0..6 {
        let angle = std::f64::consts::TAU * f64::from(k) / 6.0;
        for z in [-0.4, 0.4] {
            points.push(DVec3::new(det::cos(angle) * 0.3, det::sin(angle) * 0.3, z));
        }
    }
    points
}

/// The hull as a flat-shaded mesh: each face a fan of triangles with the face's normal.
pub fn mesh(hull: &Hull) -> MeshData {
    let mut mesh = MeshData::default();
    for face in hull.faces() {
        let normal = face.normal.as_vec3().to_array();
        let first = u32::try_from(mesh.vertices.len()).expect("a small mesh");
        for &v in &face.ring {
            mesh.vertices.push(MeshVertex::plain(
                hull.vertices()[v].as_vec3().to_array(),
                normal,
            ));
        }
        let corners = u32::try_from(face.ring.len()).expect("a small face");
        for k in 1..corners - 1 {
            // Counter-clockwise seen from outside, as the faces' corners run.
            mesh.indices
                .extend_from_slice(&[first, first + k, first + k + 1]);
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every hull is made; its mesh has a triangle for each corner of each face but two, all
    /// turned outwards.
    #[test]
    fn the_catalogue_s_hulls_make_closed_meshes() {
        for hull in catalogue() {
            let mesh = mesh(hull);
            let triangles: usize = hull.faces().iter().map(|f| f.ring.len() - 2).sum();
            assert_eq!(mesh.indices.len(), 3 * triangles);
            for t in mesh.indices.chunks(3) {
                let p = |i: u32| {
                    let v = mesh.vertices[i as usize].position;
                    DVec3::new(f64::from(v[0]), f64::from(v[1]), f64::from(v[2]))
                };
                let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
                let n = mesh.vertices[t[0] as usize].normal;
                let normal = DVec3::new(f64::from(n[0]), f64::from(n[1]), f64::from(n[2]));
                assert!((b - a).cross(c - a).dot(normal) > 0.0, "turned outwards");
            }
        }
    }
}
