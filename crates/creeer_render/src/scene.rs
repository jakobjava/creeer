//! What to draw this frame: mesh instances with materials, plus lighting.
//!
//! Instances are described in f64 simulation space and converted to camera-relative f32
//! only when the frame is built, so they stay precise anywhere in a solar system.

use crate::mesh::MeshId;
use bytemuck::{Pod, Zeroable};
use creeer_math::Frame64;
use creeer_math::glam::{DVec3, Mat4, Vec3, Vec4};

/// "PBR-lite" surface description (ADR-0003).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    /// Base colour in sRGB, as an artist would pick it (0–1).
    pub base_color: [f32; 3],
    /// 0 = mirror-smooth, 1 = fully matte.
    pub roughness: f32,
    /// 0 = dielectric (paint, stone, plastic), 1 = bare metal.
    pub metalness: f32,
    /// Light emitted, as a multiple of the base colour (0: none). A star is drawn with it;
    /// it is unaffected by lighting and shadows.
    pub emission: f32,
    /// How long the object has weathered, in years at the reference rate (`art-style.md`
    /// §6b): each palette token ages by it at its own rate. 0: new.
    pub exposure: f32,
    /// Wear from use, 0 (none) to 1 (battered): chipped edges.
    pub wear: f32,
}

impl Material {
    /// A non-metallic material.
    pub const fn dielectric(base_color: [f32; 3], roughness: f32) -> Self {
        Self {
            base_color,
            roughness,
            metalness: 0.0,
            emission: 0.0,
            exposure: 0.0,
            wear: 0.0,
        }
    }

    /// A bare metal.
    pub const fn metal(base_color: [f32; 3], roughness: f32) -> Self {
        Self {
            base_color,
            roughness,
            metalness: 1.0,
            emission: 0.0,
            exposure: 0.0,
            wear: 0.0,
        }
    }

    /// A light source of colour `base_color` (sRGB) and `strength`.
    pub const fn emissive(base_color: [f32; 3], strength: f32) -> Self {
        Self {
            base_color,
            roughness: 1.0,
            metalness: 0.0,
            emission: strength,
            exposure: 0.0,
            wear: 0.0,
        }
    }

    /// This material on an object that has weathered `exposure` years and has `wear` from use.
    pub const fn aged(self, exposure: f32, wear: f32) -> Self {
        Self {
            exposure,
            wear,
            ..self
        }
    }
}

/// One placed copy of a mesh.
#[derive(Clone, Copy, Debug)]
pub struct Instance {
    /// Pose in simulation space.
    pub frame: Frame64,
    /// Per-axis scale applied in mesh space before rotation.
    pub scale: Vec3,
    /// Surface.
    pub material: Material,
    /// Measured value shown by an engineering overlay ([`crate::Heatmap`]), if any.
    pub heat: Option<f32>,
}

/// Marks "no overlay value" on the GPU (below any real measurement).
pub(crate) const NO_HEAT: f32 = -3.0e38;

/// Instance data as uploaded to the GPU (80 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub(crate) struct InstanceRaw {
    /// Rotation quaternion (x, y, z, w).
    pub rotation: [f32; 4],
    /// Camera-relative position (m).
    pub position: [f32; 3],
    pub metalness: f32,
    pub scale: [f32; 3],
    pub roughness: f32,
    /// Linear base colour; alpha carries the overlay value (or [`NO_HEAT`]).
    pub color: [f32; 4],
    /// The object's weathering (years) and wear (0–1); two unused.
    pub ageing: [f32; 4],
}

/// Sun and ambient light.
#[derive(Clone, Copy, Debug)]
pub struct Lighting {
    /// Unit vector pointing *towards* the sun, in simulation space.
    pub sun_direction: DVec3,
    /// Sun colour × intensity (linear).
    pub sun_color: [f32; 3],
    /// Ambient light from the sky hemisphere (linear).
    pub sky_ambient: [f32; 3],
    /// Ambient light bounced from the ground (linear).
    pub ground_ambient: [f32; 3],
    /// Local up direction (away from the planet centre), for hemispheric ambient light.
    pub up: DVec3,
    /// Distance up to which sun shadows are drawn (m); 0 disables shadows.
    pub shadow_distance: f32,
    /// Exposure multiplier applied before tonemapping.
    pub exposure: f32,
}

impl Default for Lighting {
    /// Late-morning sun.
    fn default() -> Self {
        Self {
            sun_direction: DVec3::new(0.45, 0.75, 0.35).normalize(),
            sun_color: [3.2, 3.0, 2.7],
            // Matches the visible sky so metals reflect what is above them.
            sky_ambient: [0.24, 0.38, 0.62],
            ground_ambient: [0.22, 0.21, 0.16],
            up: DVec3::Y,
            shadow_distance: 250.0,
            exposure: 1.0,
        }
    }
}

/// Everything to draw this frame, grouped by mesh.
#[derive(Clone, Debug, Default)]
pub struct Scene {
    pub(crate) batches: Vec<Vec<Instance>>,
}

impl Scene {
    /// Removes all instances (keeps allocations).
    pub fn clear(&mut self) {
        for batch in &mut self.batches {
            batch.clear();
        }
    }

    /// Adds one instance of `mesh`.
    pub fn add(&mut self, mesh: MeshId, instance: Instance) {
        let index = mesh.0 as usize;
        if self.batches.len() <= index {
            self.batches.resize_with(index + 1, Vec::new);
        }
        self.batches[index].push(instance);
    }

    /// Total number of instances.
    pub fn instance_count(&self) -> usize {
        self.batches.iter().map(Vec::len).sum()
    }

    /// The instances of `mesh`.
    pub fn instances_of(&self, mesh: MeshId) -> &[Instance] {
        self.batches.get(mesh.0 as usize).map_or(&[], Vec::as_slice)
    }
}

/// Converts an sRGB-encoded channel (0–1) to linear light.
pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

impl InstanceRaw {
    pub(crate) fn new(instance: &Instance, camera: DVec3) -> Self {
        let m = instance.material;
        let [r, g, b] = m.base_color.map(srgb_to_linear);
        Self {
            rotation: instance.frame.rotation.as_quat().to_array(),
            position: (instance.frame.position - camera).as_vec3().to_array(),
            // Emission travels in the metalness slot: values from 2 up mean "emits w − 2".
            metalness: if m.emission > 0.0 {
                2.0 + m.emission
            } else {
                m.metalness
            },
            scale: instance.scale.to_array(),
            roughness: m.roughness,
            color: [r, g, b, instance.heat.unwrap_or(NO_HEAT)],
            ageing: [m.exposure, m.wear, 0.0, 0.0],
        }
    }
}

/// View frustum planes for culling in camera-relative space. With an infinite projection
/// there is no far plane.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Frustum {
    planes: [Vec4; 5],
}

impl Frustum {
    /// Extracts the planes from a view-projection matrix (Gribb–Hartmann). For wgpu's [0, 1]
    /// depth range with reversed-Z, the near plane is `row3 − row2` (depth ≤ 1).
    pub fn from_view_projection(m: Mat4) -> Self {
        let (r0, r1, r2, r3) = (m.row(0), m.row(1), m.row(2), m.row(3));
        let normalize = |p: Vec4| p / p.truncate().length();
        Self {
            planes: [
                normalize(r3 + r0), // left
                normalize(r3 - r0), // right
                normalize(r3 + r1), // bottom
                normalize(r3 - r1), // top
                normalize(r3 - r2), // near
            ],
        }
    }

    /// True if a sphere at camera-relative `center` might be visible.
    pub fn intersects_sphere(&self, center: Vec3, radius: f32) -> bool {
        self.planes
            .iter()
            .all(|p| p.truncate().dot(center) + p.w >= -radius)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Camera;

    fn frustum() -> Frustum {
        let camera = Camera::new(DVec3::ZERO);
        Frustum::from_view_projection(camera.view_projection(16.0 / 9.0))
    }

    #[test]
    fn frustum_keeps_what_is_in_front_and_culls_the_rest() {
        let f = frustum();
        assert!(f.intersects_sphere(Vec3::new(0.0, 0.0, -10.0), 1.0));
        assert!(
            f.intersects_sphere(Vec3::new(0.0, 0.0, -1.0e7), 1.0),
            "no far plane"
        );
        assert!(
            !f.intersects_sphere(Vec3::new(0.0, 0.0, 10.0), 1.0),
            "behind"
        );
        assert!(
            !f.intersects_sphere(Vec3::new(100.0, 0.0, -10.0), 1.0),
            "far right"
        );
        assert!(
            !f.intersects_sphere(Vec3::new(0.0, 100.0, -10.0), 1.0),
            "far above"
        );
    }

    #[test]
    fn frustum_keeps_spheres_straddling_an_edge() {
        let f = frustum();
        // 70° vertical FOV: at 10 m the top edge is about 7 m up.
        assert!(f.intersects_sphere(Vec3::new(0.0, 7.5, -10.0), 1.0));
        assert!(!f.intersects_sphere(Vec3::new(0.0, 9.0, -10.0), 1.0));
    }

    #[test]
    fn srgb_conversion_matches_reference_points() {
        assert!(srgb_to_linear(0.0).abs() < 1e-7);
        assert!((srgb_to_linear(1.0) - 1.0).abs() < 1e-6);
        // sRGB 0.5 is about 21.4 % linear light.
        assert!((srgb_to_linear(0.5) - 0.214_04).abs() < 1e-4);
    }

    #[test]
    fn instances_are_converted_camera_relative() {
        let camera = DVec3::new(0.0, 500_000.0, 0.0);
        let instance = Instance {
            frame: Frame64::from_position(camera + DVec3::new(1.5, -2.0, 3.25)),
            scale: Vec3::ONE,
            material: Material::dielectric([1.0, 1.0, 1.0], 0.5),
            heat: None,
        };
        let raw = InstanceRaw::new(&instance, camera);
        assert_eq!(raw.position, [1.5, -2.0, 3.25]);
        assert_eq!(raw.ageing, [0.0; 4], "new by default");
        let old = InstanceRaw::new(
            &Instance {
                material: instance.material.aged(3.5, 0.25),
                ..instance
            },
            camera,
        );
        assert_eq!(old.ageing, [3.5, 0.25, 0.0, 0.0]);
        assert_eq!(std::mem::size_of::<InstanceRaw>(), 80);
    }
}
