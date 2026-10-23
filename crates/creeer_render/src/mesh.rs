//! Triangle meshes: procedural primitives and the GPU mesh arena.
//!
//! Winding convention: counter-clockwise triangles face outwards (wgpu's default front face),
//! so back faces can be culled.

pub use crate::primitives::Round;
pub use crate::vertex::{MeshVertex, NO_TOKEN};
use creeer_math::glam::Vec3;

/// CPU-side mesh.
#[derive(Clone, Debug, Default)]
pub struct MeshData {
    /// Vertices.
    pub vertices: Vec<MeshVertex>,
    /// Triangle list (counter-clockwise = front).
    pub indices: Vec<u32>,
}

impl MeshData {
    fn push_vertex(&mut self, position: Vec3, normal: Vec3) -> u32 {
        let index = u32::try_from(self.vertices.len()).expect("mesh too large");
        self.vertices
            .push(MeshVertex::plain(position.to_array(), normal.to_array()));
        index
    }

    /// A model read from the packs, its tokens indices into the palette the renderer holds.
    pub fn from_model(model: &creeer_art::model::Model) -> Self {
        Self {
            vertices: model.vertices.iter().map(MeshVertex::from_model).collect(),
            indices: model.indices.clone(),
        }
    }

    /// Adds a planar quad with corners in counter-clockwise order as seen from the front.
    fn push_quad(&mut self, corners: [Vec3; 4], normal: Vec3) {
        let [a, b, c, d] = corners.map(|p| self.push_vertex(p, normal));
        self.indices.extend_from_slice(&[a, b, c, a, c, d]);
    }

    /// Axis-aligned box centred on the origin with flat-shaded faces.
    pub fn cuboid(half_extents: Vec3) -> Self {
        let h = half_extents;
        let mut mesh = Self::default();
        // For each face: outward normal and two in-plane axes (u × v = normal).
        for (normal, u, v) in [
            (Vec3::X, Vec3::NEG_Z, Vec3::Y),
            (Vec3::NEG_X, Vec3::Z, Vec3::Y),
            (Vec3::Y, Vec3::X, Vec3::NEG_Z),
            (Vec3::NEG_Y, Vec3::X, Vec3::Z),
            (Vec3::Z, Vec3::X, Vec3::Y),
            (Vec3::NEG_Z, Vec3::NEG_X, Vec3::Y),
        ] {
            let center = normal * h;
            let (du, dv) = (u * h, v * h);
            mesh.push_quad(
                [
                    center - du - dv,
                    center + du - dv,
                    center + du + dv,
                    center - du + dv,
                ],
                normal,
            );
        }
        mesh
    }

    /// Sphere with smooth normals, built from `segments` around and `rings` from pole to pole.
    pub fn uv_sphere(radius: f32, segments: u32, rings: u32) -> Self {
        let (segments, rings) = (segments.max(3), rings.max(2));
        let mut mesh = Self::default();
        for ring in 0..=rings {
            let theta = std::f32::consts::PI * ratio(ring, rings);
            for segment in 0..=segments {
                let phi = std::f32::consts::TAU * ratio(segment, segments);
                let normal = Vec3::new(
                    theta.sin() * phi.cos(),
                    theta.cos(),
                    -theta.sin() * phi.sin(),
                );
                mesh.push_vertex(normal * radius, normal);
            }
        }
        let stride = segments + 1;
        for ring in 0..rings {
            for segment in 0..segments {
                let a = ring * stride + segment;
                let (b, c, d) = (a + stride, a + stride + 1, a + 1);
                if ring != 0 {
                    mesh.indices.extend_from_slice(&[a, b, d]);
                }
                if ring != rings - 1 {
                    mesh.indices.extend_from_slice(&[d, b, c]);
                }
            }
        }
        mesh
    }

    /// Cylinder along +Y centred on the origin, smooth sides and flat caps.
    pub fn cylinder(radius: f32, half_height: f32, segments: u32) -> Self {
        let segments = segments.max(3);
        let mut mesh = Self::default();
        let point = |i: u32| {
            let phi = std::f32::consts::TAU * ratio(i, segments);
            Vec3::new(phi.cos(), 0.0, -phi.sin())
        };
        for i in 0..segments {
            let (n0, n1) = (point(i), point(i + 1));
            let (p0, p1) = (n0 * radius, n1 * radius);
            let up = Vec3::Y * half_height;
            // Side: two smooth-shaded triangles.
            let a = mesh.push_vertex(p0 - up, n0);
            let b = mesh.push_vertex(p1 - up, n1);
            let c = mesh.push_vertex(p1 + up, n1);
            let d = mesh.push_vertex(p0 + up, n0);
            mesh.indices.extend_from_slice(&[a, b, c, a, c, d]);
            // Caps: one triangle each from the centre.
            let top = [up, p0 + up, p1 + up].map(|p| mesh.push_vertex(p, Vec3::Y));
            mesh.indices.extend_from_slice(&top);
            let bottom = [-up, p1 - up, p0 - up].map(|p| mesh.push_vertex(p, Vec3::NEG_Y));
            mesh.indices.extend_from_slice(&bottom);
        }
        mesh
    }

    /// Radius of the smallest origin-centred sphere containing the mesh (for culling).
    pub fn bounding_radius(&self) -> f32 {
        self.vertices
            .iter()
            .map(|v| Vec3::from(v.position).length())
            .fold(0.0, f32::max)
    }
}

fn ratio(i: u32, n: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)] // small mesh resolutions
    let r = i as f32 / n as f32;
    r
}

/// Handle to a mesh registered in a [`MeshArena`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MeshId(pub(crate) u32);

/// Where one mesh lives inside the shared buffers.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MeshRange {
    pub first_index: u32,
    pub index_count: u32,
    pub base_vertex: i32,
    pub bounding_radius: f32,
    /// A model's (its vertices name palette tokens): drawn with the full surface.
    pub model: bool,
    /// For a level of detail, its silhouette error as a share of the shape's radius.
    pub error: f32,
}

/// One registered mesh: its levels of detail among the arena's ranges, coarsest first.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Levels {
    pub first: usize,
    pub count: usize,
    /// How its error is measured, when it has levels.
    pub round: Option<Round>,
}

/// All meshes in one vertex and one index buffer (ADR-0003).
#[derive(Debug, Default)]
pub struct MeshArena {
    vertices: Vec<MeshVertex>,
    indices: Vec<u32>,
    pub(crate) ranges: Vec<MeshRange>,
    pub(crate) meshes: Vec<Levels>,
    dirty: bool,
}

impl MeshArena {
    /// Registers a mesh; it is uploaded before the next frame.
    pub fn add(&mut self, mesh: &MeshData) -> MeshId {
        let first = self.push_range(mesh, 0.0);
        self.register(Levels {
            first,
            count: 1,
            round: None,
        })
    }

    /// Registers a round shape's levels of detail, coarsest first, each with its silhouette
    /// error as a share of its radius (`crate::primitives`). Each instance is drawn with the
    /// coarsest whose error projects under half a pixel.
    pub fn add_levels(&mut self, levels: &[(MeshData, f32)], round: Round) -> MeshId {
        assert!(!levels.is_empty(), "a mesh has at least one level");
        let first = self.ranges.len();
        for (mesh, error) in levels {
            self.push_range(mesh, *error);
        }
        self.register(Levels {
            first,
            count: levels.len(),
            round: Some(round),
        })
    }

    fn push_range(&mut self, mesh: &MeshData, error: f32) -> usize {
        self.ranges.push(MeshRange {
            first_index: u32::try_from(self.indices.len()).expect("arena too large"),
            index_count: u32::try_from(mesh.indices.len()).expect("mesh too large"),
            base_vertex: i32::try_from(self.vertices.len()).expect("arena too large"),
            bounding_radius: mesh.bounding_radius(),
            model: mesh
                .vertices
                .iter()
                .any(|v| v.material & 0xffff != u32::from(NO_TOKEN)),
            error,
        });
        self.vertices.extend_from_slice(&mesh.vertices);
        self.indices.extend_from_slice(&mesh.indices);
        self.dirty = true;
        self.ranges.len() - 1
    }

    fn register(&mut self, levels: Levels) -> MeshId {
        self.meshes.push(levels);
        MeshId(u32::try_from(self.meshes.len() - 1).expect("too many meshes"))
    }

    /// Number of registered meshes.
    pub fn len(&self) -> usize {
        self.meshes.len()
    }

    /// True if no mesh is registered.
    pub fn is_empty(&self) -> bool {
        self.meshes.is_empty()
    }

    /// Returns the data to upload if it changed since the last call.
    pub(crate) fn take_upload(&mut self) -> Option<(&[MeshVertex], &[u32])> {
        if !self.dirty {
            return None;
        }
        self.dirty = false;
        Some((&self.vertices, &self.indices))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// For a convex mesh around the origin, every triangle must face away from the centre,
    /// and every vertex normal must be unit length.
    fn assert_convex_outward(mesh: &MeshData) {
        for v in &mesh.vertices {
            let n = Vec3::from(v.normal);
            assert!((n.length() - 1.0).abs() < 1e-5, "normal {n} not unit");
        }
        for tri in mesh.indices.as_chunks::<3>().0 {
            let [a, b, c] = tri.map(|i| Vec3::from(mesh.vertices[i as usize].position));
            let face = (b - a).cross(c - a);
            let centroid = (a + b + c) / 3.0;
            assert!(
                face.dot(centroid) > 0.0,
                "triangle {a} {b} {c} faces inwards"
            );
        }
    }

    #[test]
    fn cuboid_is_closed_and_faces_outwards() {
        let mesh = MeshData::cuboid(Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(mesh.indices.len(), 36);
        assert_convex_outward(&mesh);
        let r = mesh.bounding_radius();
        assert!((r - 14f32.sqrt()).abs() < 1e-5);
    }

    #[test]
    fn sphere_faces_outwards_with_radial_normals() {
        let mesh = MeshData::uv_sphere(2.0, 24, 12);
        assert_convex_outward(&mesh);
        for v in &mesh.vertices {
            let (p, n) = (Vec3::from(v.position), Vec3::from(v.normal));
            assert!((p.length() - 2.0).abs() < 1e-5);
            assert!(n.dot(p.normalize()) > 0.9999);
        }
        // 24 × (12 − 2) quads plus 2 × 24 pole triangles.
        assert_eq!(mesh.indices.len() / 3, 24 * 10 * 2 + 2 * 24);
    }

    #[test]
    fn cylinder_faces_outwards() {
        let mesh = MeshData::cylinder(0.5, 1.0, 16);
        assert_convex_outward(&mesh);
        assert_eq!(mesh.indices.len() / 3, 16 * 4);
    }

    #[test]
    fn arena_records_ranges_and_upload_once() {
        let mut arena = MeshArena::default();
        let a = arena.add(&MeshData::cuboid(Vec3::ONE));
        let b = arena.add(&MeshData::cylinder(1.0, 1.0, 8));
        assert_ne!(a, b);
        let range_b = arena.ranges[b.0 as usize];
        assert_eq!(range_b.first_index, 36);
        assert_eq!(range_b.base_vertex, 24);
        assert!(
            !range_b.model,
            "procedural meshes take their instance's material"
        );
        let mut model = MeshData::cuboid(Vec3::ONE);
        model.vertices[0].material = crate::vertex::pack_material(3, 1.0, 0.0);
        let c = arena.add(&model);
        assert!(
            arena.ranges[c.0 as usize].model,
            "a vertex naming a token makes a model"
        );
        // A round shape's levels: one mesh, a range for each level, after the others.
        let d = arena.add_levels(&crate::primitives::cylinder_levels(), Round::Cylinder);
        let levels = arena.meshes[d.0 as usize];
        assert_eq!((levels.first, levels.count), (3, 10));
        assert_eq!(arena.len(), 4);
        assert!((arena.ranges[3].error - crate::primitives::sagitta(6)).abs() < 1e-7);
        assert!(arena.take_upload().is_some());
        assert!(arena.take_upload().is_none(), "uploads only when changed");
    }
}
