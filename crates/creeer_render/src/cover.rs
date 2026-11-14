//! Decoratives on the ground (G2.11, ADR-0037): grass tufts and grit, many of a kind, drawn
//! instanced near the camera (`renderer/cover_pass.rs`, `cover.wgsl`). The client places them
//! (`creeer_client::ground_cover`); this is what the GPU is given.

use bytemuck::{Pod, Zeroable};
use std::f32::consts::TAU;

/// The shapes a decorative is drawn as, in the order their instances are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoverShape {
    /// Blades from one root, leaning out.
    Tuft = 0,
    /// A rounded stone half in the ground.
    Pebble = 1,
}

/// One decorative as the GPU draws it (32 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct CoverInstance {
    /// Where its root stands, camera-relative (m).
    pub position: [f32; 3],
    /// Its height (m).
    pub height: f32,
    /// Its width (m).
    pub width: f32,
    /// The ground's normal there (snorm xyz) and its turn about it (w: −1..1 of a half turn).
    pub up: [i8; 4],
    /// Its colour (sRGB) and its tone (w: 0–255, ±10 % about 128).
    pub colour: [u8; 4],
    /// How far its blades lean out (0–1).
    pub lean: f32,
}

/// A vertex of a decorative's shape, in its own frame: x and z across (times its width), y up
/// (times its height).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct CoverVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    /// From its root (0) to its tip (1).
    pub along: f32,
}

/// Blades in a tuft.
const BLADES: u32 = 7;
/// Segments of a blade, root to tip.
const SEGMENTS: u32 = 3;
/// A blade's width at its root, in the tuft's widths (a tuft 10 cm wide: blades of 6 mm).
const BLADE_WIDTH: f32 = 0.06;

/// A tuft: `BLADES` tapering blades around its root, each leaning out and curving (the shader
/// leans them by the instance's `lean`). Normals lean up, so a tuft is lit as the ground it
/// stands on is, not as flat cards.
pub fn tuft() -> (Vec<CoverVertex>, Vec<u32>) {
    let (mut vertices, mut indices) = (Vec::new(), Vec::new());
    for b in 0..BLADES {
        #[allow(clippy::cast_precision_loss)] // a few blades
        let k = b as f32;
        // Spread round by the golden angle; rooted a little apart; some shorter.
        let angle = k * 2.399_963;
        let (s, c) = angle.sin_cos();
        let root = [0.12 * c * (0.4 + 0.6 * (k * 0.37).fract()), 0.0, 0.12 * s];
        let tall = 0.65 + 0.35 * (k * 0.618).fract();
        let out = [c, 0.0, s];
        let side = [-s, 0.0, c];
        let first = u32::try_from(vertices.len()).expect("small mesh");
        for j in 0..=SEGMENTS {
            #[allow(clippy::cast_precision_loss)]
            let t = j as f32 / SEGMENTS as f32;
            // Out along a curve that bends further near the tip.
            let reach = 0.3 * t * t;
            let centre = [root[0] + out[0] * reach, tall * t, root[2] + out[2] * reach];
            let half = 0.5 * BLADE_WIDTH * (1.0 - t);
            let normal = normalize([out[0] * 0.5, 0.85, out[2] * 0.5]);
            for sign in [-1.0f32, 1.0] {
                if j == SEGMENTS && sign > 0.0 {
                    break; // the tip is one point
                }
                let w = if j == SEGMENTS { 0.0 } else { half * sign };
                vertices.push(CoverVertex {
                    position: [centre[0] + side[0] * w, centre[1], centre[2] + side[2] * w],
                    normal,
                    along: t,
                });
            }
        }
        for j in 0..SEGMENTS - 1 {
            let a = first + 2 * j;
            indices.extend([a, a + 1, a + 2, a + 1, a + 3, a + 2]);
        }
        let last = first + 2 * (SEGMENTS - 1);
        indices.extend([last, last + 1, last + 2]);
    }
    (vertices, indices)
}

/// Around a pebble.
const PEBBLE_SIDES: u32 = 8;

/// A pebble: a rounded dome sunk a third into the ground, smooth-shaded.
pub fn pebble() -> (Vec<CoverVertex>, Vec<u32>) {
    let mut vertices = Vec::new();
    let rings = [(-0.35f32, 0.5f32), (0.4, 0.46), (0.85, 0.26)];
    for &(y, r) in &rings {
        for k in 0..PEBBLE_SIDES {
            #[allow(clippy::cast_precision_loss)]
            let a = k as f32 / PEBBLE_SIDES as f32 * TAU;
            let (s, c) = a.sin_cos();
            vertices.push(CoverVertex {
                position: [r * c, y, r * s],
                normal: normalize([c, y.max(0.0) * 1.6, s]),
                along: (y + 0.35) / 1.35,
            });
        }
    }
    let top = u32::try_from(vertices.len()).expect("small mesh");
    vertices.push(CoverVertex {
        position: [0.0, 1.0, 0.0],
        normal: [0.0, 1.0, 0.0],
        along: 1.0,
    });
    let mut indices = Vec::new();
    let n = PEBBLE_SIDES;
    for ring in 0..2 {
        for k in 0..n {
            let (a, b) = (ring * n + k, ring * n + (k + 1) % n);
            let (c, d) = (a + n, b + n);
            indices.extend([a, c, b, b, c, d]);
        }
    }
    for k in 0..n {
        indices.extend([2 * n + k, top, 2 * n + (k + 1) % n]);
    }
    (vertices, indices)
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-9);
    [v[0] / l, v[1] / l, v[2] / l]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instances_pack_into_thirty_two_bytes() {
        assert_eq!(std::mem::size_of::<CoverInstance>(), 32);
        assert_eq!(std::mem::size_of::<CoverVertex>(), 28);
    }

    /// Both shapes are small (art-style §7: a decorative ≤ 200 triangles), every index names a
    /// vertex, every normal is a unit vector facing up more than down, and nothing stands
    /// outside the unit footprint or above its height.
    #[test]
    fn the_shapes_are_small_and_whole() {
        for (vertices, indices) in [tuft(), pebble()] {
            assert_eq!(indices.len() % 3, 0);
            assert!(indices.len() / 3 <= 200, "{} triangles", indices.len() / 3);
            assert!(indices.iter().all(|&i| (i as usize) < vertices.len()));
            for v in &vertices {
                let [x, y, z] = v.position;
                let n = v.normal;
                let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                assert!((len - 1.0).abs() < 1e-5 && n[1] >= 0.0, "{v:?}");
                assert!(x.abs() <= 0.5 && z.abs() <= 0.5 && y <= 1.0 + 1e-6, "{v:?}");
                assert!((0.0..=1.0).contains(&v.along));
            }
        }
    }
}
