//! Levels of detail chosen by their error on screen (G1.3c, ADR-0026 decision 11). Each
//! instance of a primitive is drawn with the coarsest level whose silhouette error,
//! projected at the instance's nearest point, stays under half a pixel (after Hoppe's
//! screen-space error, 1997). A switch then moves the silhouette by less than half a pixel, so
//! nothing pops and no geomorphs are needed.
//!
//! Its shadows are drawn at their own level in each cascade: the coarsest under half a texel
//! of its map (as Nanite does). A coarser caster lies within the drawn shape (its corners on
//! the true surface), so it casts no shadow on the shape itself.

use crate::mesh::MeshRange;
use crate::primitives::{Round, bevel_radius};
use creeer_math::glam::{Quat, Vec3};

/// The largest error a silhouette may have on screen (pixels), or in a shadow map (texels).
pub(crate) const HALF_PIXEL: f32 = 0.5;

/// The length an instance's error is a share of: its bevel radius (the box) or its radius.
pub(crate) fn round_radius(round: Round, scale: Vec3) -> f32 {
    match round {
        Round::Box => bevel_radius(scale),
        Round::Cylinder => 0.5 * scale.x.max(scale.z),
        Round::Sphere => 0.5 * scale.max_element(),
    }
}

/// The level (an index into `levels`, coarsest first) an instance of `round` is drawn with, at
/// camera-relative `position`, turned by `rotation` and scaled by `scale`. `focal` is the
/// viewport's height in pixels over 2·tan(fov/2): a length l at distance d spans l·focal/d
/// pixels.
pub(crate) fn level(
    levels: &[MeshRange],
    round: Round,
    position: Vec3,
    rotation: Quat,
    scale: Vec3,
    focal: f32,
) -> usize {
    let radius = round_radius(round, scale);
    // How far its nearest point is.
    let distance = match round {
        Round::Box => {
            // The camera in the box's own frame, and how far it is outside the box.
            let camera = rotation.inverse() * -position;
            (camera.abs() - 0.5 * scale).max(Vec3::ZERO).length()
        }
        Round::Cylinder => {
            // The nearest point of its axis to the camera (at the origin), less its radius.
            let half = rotation * Vec3::new(0.0, 0.5 * scale.y, 0.0);
            let t = (-position.dot(half) / half.length_squared().max(1e-12)).clamp(-1.0, 1.0);
            (position + half * t).length() - radius
        }
        Round::Sphere => position.length() - radius,
    };
    if distance <= 0.0 && radius > 0.0 {
        return levels.len() - 1;
    }
    coarsest_within(levels, radius, HALF_PIXEL * distance / focal)
}

/// The level a shadow cascade with texels of `texel` metres draws an instance at whose
/// error is a share of `radius`.
pub(crate) fn shadow_level(levels: &[MeshRange], radius: f32, texel: f32) -> usize {
    coarsest_within(levels, radius, HALF_PIXEL * texel)
}

/// The coarsest level whose error, a share of `radius`, is within `allowed` metres (the
/// finest when none is).
fn coarsest_within(levels: &[MeshRange], radius: f32, allowed: f32) -> usize {
    if radius <= 0.0 {
        return 0;
    }
    levels
        .iter()
        .position(|range| range.error * radius <= allowed)
        .unwrap_or(levels.len() - 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::{LEVELS, sagitta};

    fn ranges() -> Vec<MeshRange> {
        LEVELS
            .iter()
            .map(|&n| MeshRange {
                first_index: 0,
                index_count: 0,
                base_vertex: 0,
                bounding_radius: 1.0,
                model: false,
                error: sagitta(n),
            })
            .collect()
    }

    #[test]
    fn each_instance_gets_the_coarsest_level_under_half_a_pixel() {
        let levels = ranges();
        // 1080 pixels high, 70° field of view.
        let focal = 1080.0 / (2.0 * 35f32.to_radians().tan());
        let scale = Vec3::new(0.2, 4.0, 0.2);
        for distance in [0.3, 1.0, 3.0, 10.0, 30.0, 100.0, 1000.0] {
            // Upright, straight ahead: its axis's nearest point is `distance` away.
            let k = level(
                &levels,
                Round::Cylinder,
                Vec3::new(0.0, 0.0, -distance),
                Quat::IDENTITY,
                scale,
                focal,
            );
            let pixels = |k: usize| levels[k].error * 0.1 * focal / (distance - 0.1);
            assert!(
                k == levels.len() - 1 || pixels(k) <= HALF_PIXEL,
                "{distance}: {k}"
            );
            assert!(
                k == 0 || pixels(k - 1) > HALF_PIXEL,
                "{distance}: {k} is not the coarsest"
            );
        }
        // Lying along the view, 4 m long, its near end 0.5 m away: drawn as finely as one
        // standing 0.5 m away, not as one at its centre's 2.5 m.
        let lying = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
        let at = |z: f32, rotation| {
            level(
                &levels,
                Round::Cylinder,
                Vec3::new(0.0, 0.0, z),
                rotation,
                scale,
                focal,
            )
        };
        assert_eq!(at(-2.5, lying), at(-0.5, Quat::IDENTITY));
        assert!(at(-2.5, lying) > at(-2.5, Quat::IDENTITY));
        // Inside it, or too thin to see.
        assert_eq!(
            level(
                &levels,
                Round::Sphere,
                Vec3::ZERO,
                Quat::IDENTITY,
                Vec3::ONE,
                focal
            ),
            9
        );
        assert_eq!(
            level(
                &levels,
                Round::Sphere,
                Vec3::Z,
                Quat::IDENTITY,
                Vec3::ZERO,
                focal
            ),
            0
        );
    }

    #[test]
    fn shadows_take_the_coarsest_level_under_half_a_texel() {
        let levels = ranges();
        // A pipe 0.2 m across in a cascade of 2 cm texels: under 1 cm of error, 0.1 of its
        // radius, so 8 segments (0.076); in one of 20 cm texels, the coarsest.
        let radius = round_radius(Round::Cylinder, Vec3::new(0.2, 4.0, 0.2));
        assert!((radius - 0.1).abs() < 1e-7);
        assert_eq!(LEVELS[shadow_level(&levels, radius, 0.02)], 8);
        assert_eq!(shadow_level(&levels, radius, 0.2), 0);
        // A 1 m box: its error a share of its 2.5 cm bevel radius.
        assert!((round_radius(Round::Box, Vec3::ONE) - 0.025).abs() < 1e-7);
        // Finer texels, finer levels: never coarser.
        let mut last = 0;
        for texel in [0.5, 0.1, 0.03, 0.01, 0.003, 0.001] {
            let k = shadow_level(&levels, 1.0, texel);
            assert!(k >= last);
            last = k;
        }
    }
}
