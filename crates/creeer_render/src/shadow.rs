//! Cascaded shadow maps for the sun.
//!
//! The view frustum is cut into [`CASCADES`] slices; each gets its own orthographic shadow
//! map, small (sharp) near the camera and large far away. Two details matter:
//!
//! - **Bounding spheres:** each cascade covers a sphere around its slice instead of a tight
//!   box, so the cascade size does not change when the camera *rotates*.
//! - **Texel snapping in f64 world space:** the cascade centre is snapped to whole shadow-map
//!   texels of a world-fixed grid, so shadow edges do not shimmer when the camera *moves*.
//!   Snapping must use world (not camera-relative) coordinates, or the grid would move along
//!   with the camera.

use crate::camera::Camera;
use creeer_math::glam::{DVec3, Mat4, Vec3, Vec4};

/// Number of shadow cascades.
pub(crate) const CASCADES: usize = 4;
/// Resolution of each shadow cascade. Memory for all four cascades (32-bit depth):
/// Low 16 MB, Medium 64 MB, High 256 MB. Medium suits 4 GB graphics cards.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShadowQuality {
    /// 1024² per cascade.
    Low,
    /// 2048² per cascade.
    Medium,
    /// 4096² per cascade.
    #[default]
    High,
}

impl ShadowQuality {
    /// Edge length of one cascade's shadow map in texels.
    pub fn map_size(self) -> u32 {
        match self {
            Self::Low => 1024,
            Self::Medium => 2048,
            Self::High => 4096,
        }
    }
}
/// How far behind a cascade (towards the sun) casters are still captured (m).
const CASTER_RANGE: f32 = 500.0;
/// Mix between logarithmic (1.0) and uniform (0.0) split distribution. Higher values favour
/// the near cascades at the expense of the far ones. Measured: 0.85 sharpened close range
/// but made shadows 70–100 m away coarser than 0.75; 0.8 balances both.
const SPLIT_LAMBDA: f32 = 0.8;
/// Casters smaller than this many shadow-map texels are skipped: in a coarse cascade they
/// can only flicker in and out.
const MIN_CASTER_TEXELS: f32 = 1.0;
/// Distance where the split computation starts (the camera's own near plane is tiny).
const SPLIT_START: f32 = 0.5;

/// One cascade, in camera-relative space.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Cascade {
    /// Camera-relative position → shadow clip space (x, y in [−1, 1], z in [0, 1]).
    pub view_proj: Mat4,
    /// Centre of the covered sphere (camera-relative, m).
    pub center: Vec3,
    /// Radius of the covered sphere (m).
    pub radius: f32,
    /// Far end of this cascade's slice along the view direction (m).
    pub far: f32,
    /// Size of one shadow-map texel in the world (m).
    pub texel: f32,
}

/// Orthonormal basis of the sun's view, fixed in world space.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LightBasis {
    pub right: DVec3,
    pub up: DVec3,
    /// Unit vector towards the sun.
    pub to_sun: DVec3,
}

impl LightBasis {
    pub fn new(to_sun: DVec3) -> Self {
        let to_sun = to_sun.normalize();
        // Any reference not parallel to the sun direction works; it must only be stable.
        let reference = if to_sun.y.abs() < 0.99 {
            DVec3::Y
        } else {
            DVec3::X
        };
        let right = reference.cross(to_sun).normalize();
        let up = to_sun.cross(right);
        Self { right, up, to_sun }
    }
}

/// Split distances (far end of each cascade) between the camera and `distance`.
pub(crate) fn split_distances(distance: f32) -> [f32; CASCADES] {
    std::array::from_fn(|i| {
        #[allow(clippy::cast_precision_loss)]
        let t = (i + 1) as f32 / CASCADES as f32;
        let logarithmic = SPLIT_START * (distance / SPLIT_START).powf(t);
        let uniform = SPLIT_START + (distance - SPLIT_START) * t;
        SPLIT_LAMBDA * logarithmic + (1.0 - SPLIT_LAMBDA) * uniform
    })
}

/// Smallest sphere containing the frustum slice between view distances `near` and `far`,
/// as (distance of the centre along the view axis, radius). `k` is the slope of the slice's
/// corner rays: corner offset from the axis = k × distance.
pub(crate) fn slice_sphere(near: f32, far: f32, k: f32) -> (f32, f32) {
    let k2 = k * k;
    if k2 >= (far - near) / (far + near) {
        (far, far * k)
    } else {
        let center = 0.5 * (far + near) * (1.0 + k2);
        let radius = 0.5
            * ((far - near) * (far - near)
                + 2.0 * (far * far + near * near) * k2
                + (far + near) * (far + near) * k2 * k2)
                .sqrt();
        (center, radius)
    }
}

/// Computes all cascades for this frame.
pub(crate) fn compute_cascades(
    camera: &Camera,
    aspect: f32,
    basis: &LightBasis,
    shadow_distance: f32,
    map_size: u32,
) -> [Cascade; CASCADES] {
    let splits = split_distances(shadow_distance);
    let tan_half = (camera.fov_y * 0.5).tan();
    let k = tan_half * (1.0 + aspect * aspect).sqrt();
    let forward = camera.forward();
    std::array::from_fn(|i| {
        let near = if i == 0 { camera.near } else { splits[i - 1] };
        let far = splits[i];
        let (along, radius) = slice_sphere(near, far, k);
        // Round the radius up so the texel size is identical from frame to frame.
        let radius = radius.ceil();
        let texel = 2.0 * radius / f32_from(map_size);

        // Snap the centre to the world-fixed texel grid (f64: the world can be huge).
        let center_world = camera.position + forward * f64::from(along);
        let snap = |coordinate: f64| (coordinate / f64::from(texel)).floor() * f64::from(texel);
        let u = center_world.dot(basis.right);
        let v = center_world.dot(basis.up);
        let snapped_world = center_world + basis.right * (snap(u) - u) + basis.up * (snap(v) - v);
        let center = (snapped_world - camera.position).as_vec3();

        Cascade {
            view_proj: light_view_projection(basis, center, radius),
            center,
            radius,
            far,
            texel,
        }
    })
}

/// Orthographic projection of the sun's view around `center` (camera-relative).
fn light_view_projection(basis: &LightBasis, center: Vec3, radius: f32) -> Mat4 {
    let (right, up, to_sun) = (
        basis.right.as_vec3(),
        basis.up.as_vec3(),
        basis.to_sun.as_vec3(),
    );
    // Depth 0 at the plane CASTER_RANGE towards the sun, 1 at the far side of the sphere.
    let near_point = center + to_sun * CASTER_RANGE;
    let range = CASTER_RANGE + radius;
    let rows = [
        (right / radius).extend(-center.dot(right) / radius),
        (up / radius).extend(-center.dot(up) / radius),
        (-to_sun / range).extend(near_point.dot(to_sun) / range),
        Vec4::W,
    ];
    Mat4::from_cols_array_2d(&[
        [rows[0].x, rows[1].x, rows[2].x, rows[3].x],
        [rows[0].y, rows[1].y, rows[2].y, rows[3].y],
        [rows[0].z, rows[1].z, rows[2].z, rows[3].z],
        [rows[0].w, rows[1].w, rows[2].w, rows[3].w],
    ])
}

impl Cascade {
    /// True if a sphere (camera-relative centre) may cast a shadow into this cascade: it
    /// is at least a texel across, overlaps the cascade sideways and is not entirely beyond
    /// it (seen from the sun).
    pub fn may_cast_into(&self, basis: &LightBasis, center: Vec3, radius: f32) -> bool {
        let offset = center - self.center;
        let reach = self.radius + radius;
        2.0 * radius >= MIN_CASTER_TEXELS * self.texel
            && offset.dot(basis.right.as_vec3()).abs() <= reach
            && offset.dot(basis.up.as_vec3()).abs() <= reach
            && offset.dot(basis.to_sun.as_vec3()) >= -reach
            && offset.dot(basis.to_sun.as_vec3()) <= CASTER_RANGE + radius
    }
}

fn f32_from(value: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)] // shadow map sizes are small
    let v = value as f32;
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_MAP_SIZE: u32 = 2048;

    fn camera_at(position: DVec3) -> Camera {
        let mut camera = Camera::new(position);
        camera.yaw = 0.7;
        camera.pitch = -0.3;
        camera
    }

    fn sun() -> LightBasis {
        LightBasis::new(DVec3::new(0.45, 0.75, 0.35))
    }

    #[test]
    fn basis_is_orthonormal_even_for_a_vertical_sun() {
        for to_sun in [
            DVec3::new(0.45, 0.75, 0.35),
            DVec3::Y,
            DVec3::NEG_Y,
            DVec3::X,
        ] {
            let b = LightBasis::new(to_sun);
            for v in [b.right, b.up, b.to_sun] {
                assert!((v.length() - 1.0).abs() < 1e-12);
            }
            assert!(b.right.dot(b.up).abs() < 1e-12);
            assert!(b.right.dot(b.to_sun).abs() < 1e-12);
            assert!(b.up.dot(b.to_sun).abs() < 1e-12);
        }
    }

    #[test]
    fn splits_grow_and_end_at_the_shadow_distance() {
        let splits = split_distances(300.0);
        assert!(splits.windows(2).all(|w| w[0] < w[1]), "{splits:?}");
        assert!((splits[CASCADES - 1] - 300.0).abs() < 1e-3);
        assert!(
            splits[0] < 30.0,
            "first cascade should be close: {splits:?}"
        );
    }

    /// Every corner of every slice must be inside its cascade's sphere and projection.
    #[test]
    fn cascades_cover_their_view_slices() {
        let camera = camera_at(DVec3::new(10.0, 500_000.0, -3.0));
        let aspect = 16.0 / 9.0;
        let cascades = compute_cascades(&camera, aspect, &sun(), 300.0, TEST_MAP_SIZE);
        let rotation = camera.rotation().as_quat();
        let tan_half = (camera.fov_y * 0.5).tan();
        let mut near = camera.near;
        for cascade in &cascades {
            for d in [near, cascade.far] {
                let (h, w) = (d * tan_half, d * tan_half * aspect);
                for (x, y) in [(-w, -h), (w, -h), (-w, h), (w, h)] {
                    let corner = rotation * Vec3::new(x, y, -d);
                    let distance = (corner - cascade.center).length();
                    assert!(distance <= cascade.radius * 1.0001, "corner outside sphere");
                    let clip = cascade.view_proj * corner.extend(1.0);
                    assert!(clip.x.abs() <= 1.0001 && clip.y.abs() <= 1.0001, "{clip}");
                    assert!((0.0..=1.0).contains(&clip.z), "depth {}", clip.z);
                }
            }
            near = cascade.far;
        }
    }

    /// Stabilisation: moving the camera must not move the shadow-map texel grid relative to
    /// the world (no shimmering edges).
    #[test]
    fn texel_grid_is_fixed_in_the_world() {
        let basis = sun();
        let a = camera_at(DVec3::new(0.0, 2.0, 0.0));
        let b = camera_at(DVec3::new(0.37, 2.11, -0.23));
        let world_point = DVec3::new(3.3, 0.0, -7.9);
        let texel_coordinate = |camera: &Camera| {
            let cascade = compute_cascades(camera, 1.5, &basis, 300.0, TEST_MAP_SIZE)[0];
            let p = (world_point - camera.position).as_vec3();
            let clip = cascade.view_proj * p.extend(1.0);
            (clip.x * 0.5 * f32_from(TEST_MAP_SIZE)).rem_euclid(1.0)
        };
        let (fa, fb) = (texel_coordinate(&a), texel_coordinate(&b));
        let diff = (fa - fb).abs().min(1.0 - (fa - fb).abs());
        assert!(diff < 0.01, "sub-texel offset changed: {fa} vs {fb}");
    }

    #[test]
    fn rotating_the_camera_keeps_the_cascade_size() {
        let mut camera = camera_at(DVec3::ZERO);
        let first = compute_cascades(&camera, 1.7, &sun(), 300.0, TEST_MAP_SIZE);
        camera.yaw += 1.234;
        camera.pitch = 0.4;
        let second = compute_cascades(&camera, 1.7, &sun(), 300.0, TEST_MAP_SIZE);
        for (a, b) in first.iter().zip(&second) {
            assert!((a.radius - b.radius).abs() < f32::EPSILON);
            assert!((a.texel - b.texel).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn sub_texel_casters_are_skipped_only_in_coarse_cascades() {
        let basis = sun();
        let camera = camera_at(DVec3::ZERO);
        let cascades = compute_cascades(&camera, 1.7, &basis, 250.0, TEST_MAP_SIZE);
        let (fine, coarse) = (cascades[0], cascades[CASCADES - 1]);
        let bolt = 0.05; // 10 cm across
        assert!(fine.may_cast_into(&basis, fine.center, bolt));
        assert!(!coarse.may_cast_into(&basis, coarse.center, bolt));
        assert!(
            coarse.may_cast_into(&basis, coarse.center, 2.0),
            "a tank still casts"
        );
    }

    #[test]
    fn casters_between_the_sun_and_the_cascade_are_kept() {
        let basis = sun();
        let camera = camera_at(DVec3::ZERO);
        let cascade = compute_cascades(&camera, 1.7, &basis, 300.0, TEST_MAP_SIZE)[1];
        let towards_sun = cascade.center + basis.to_sun.as_vec3() * 200.0;
        assert!(cascade.may_cast_into(&basis, towards_sun, 1.0));
        let far_aside = cascade.center + basis.right.as_vec3() * (cascade.radius + 50.0);
        assert!(!cascade.may_cast_into(&basis, far_aside, 1.0));
        let behind = cascade.center - basis.to_sun.as_vec3() * (cascade.radius + 50.0);
        assert!(!cascade.may_cast_into(&basis, behind, 1.0));
    }
}
