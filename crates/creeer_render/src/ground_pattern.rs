//! The ground's patches on the CPU, mirroring `ground.wgsl` (G2.11, ADR-0037): where a layer
//! (grass, moss, litter) covers the ground, so decoratives can stand where the shader draws
//! their layer (a grass tuft in a grass patch, not on bare soil beside it).
//!
//! The same noise volume (`renderer/noise.rs`) read the same way, and the same thresholds as
//! the shader, at full detail (a decorative is placed once, not seen through a pixel).

use creeer_math::glam::{DVec3, Vec3};
use std::sync::OnceLock;

/// `terrain::PATTERN_PERIOD` (1,024 m: exact in f32).
#[allow(clippy::cast_possible_truncation)]
const PATTERN_PERIOD: f32 = crate::terrain::PATTERN_PERIOD as f32;
const FINE_PERIOD: f32 = 8.0;
/// The noise volume's cells per side.
#[allow(clippy::cast_precision_loss)] // 32
const NOISE_CELLS: f32 = crate::renderer::noise::SIZE as f32;

/// The noise volume's value at lattice point `p` (wrapped), 0–1 as the GPU reads a byte.
fn lattice(p: [i32; 3]) -> f32 {
    static VALUES: OnceLock<Vec<u8>> = OnceLock::new();
    let values = VALUES.get_or_init(crate::renderer::noise::values);
    let n = i32::try_from(crate::renderer::noise::SIZE).expect("32");
    let wrap = |x: i32| usize::try_from(x.rem_euclid(n)).expect("wrapped");
    let size = 32;
    f32::from(values[wrap(p[0]) + size * (wrap(p[1]) + size * wrap(p[2]))]) / 255.0
}

/// As `noise_at` in `ground.wgsl`: the volume's trilinear filter at the quintic-warped fraction.
fn noise_at(local: Vec3, size: f32, salt: f32) -> f32 {
    let (period, p) = if size * NOISE_CELLS <= FINE_PERIOD {
        let wrapped = local - FINE_PERIOD * (local / FINE_PERIOD).floor();
        (FINE_PERIOD, wrapped)
    } else {
        (PATTERN_PERIOD, local)
    };
    let tiles = (period / (size * NOISE_CELLS)).round().max(1.0);
    let q = p * (tiles * NOISE_CELLS / period) + Vec3::new(salt * 7.0, salt * 3.0, salt * 5.0);
    let i = q.floor();
    let f = q - i;
    let u = f * f * f * (f * (f * 6.0 - Vec3::splat(15.0)) + Vec3::splat(10.0));
    #[allow(clippy::cast_possible_truncation)] // lattice coordinates of the pattern
    let c = [i.x as i32, i.y as i32, i.z as i32];
    let at = |dx: i32, dy: i32, dz: i32| lattice([c[0] + dx, c[1] + dy, c[2] + dz]);
    let mix = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let x00 = mix(at(0, 0, 0), at(1, 0, 0), u.x);
    let x10 = mix(at(0, 1, 0), at(1, 1, 0), u.x);
    let x01 = mix(at(0, 0, 1), at(1, 0, 1), u.x);
    let x11 = mix(at(0, 1, 1), at(1, 1, 1), u.x);
    mix(mix(x00, x10, u.y), mix(x01, x11, u.y), u.z)
}

/// Where `point` (body-fixed, m) lies in the ground's pattern (`terrain::pattern_offset`).
pub fn pattern_point(point: DVec3) -> Vec3 {
    let wrap = |x: f64| {
        #[allow(clippy::cast_possible_truncation)] // within the period
        let v = x.rem_euclid(crate::terrain::PATTERN_PERIOD) as f32;
        v
    };
    Vec3::new(wrap(point.x), wrap(point.y), wrap(point.z))
}

/// How thickly layer `k` (of patches `size` m and `density`) covers the ground at pattern
/// point `local` where it covers `amount` of it, as `ground_at` in `ground.wgsl` draws it.
pub fn layer_mask(local: Vec3, k: usize, (size, density): (f32, f32), amount: f32) -> f32 {
    if amount < 0.004 {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)] // four layers
    let salt = 10.0 + k as f32 * 3.0;
    let n = 0.55 * noise_at(local, size, salt)
        + 0.3 * noise_at(local, 0.31 * size, salt + 1.0)
        + 0.15 * noise_at(local, 0.07 * size, salt + 5.0);
    let threshold = 0.5 + (0.5 - amount) * 0.6;
    let soft = 0.03;
    smoothstep(threshold - soft, threshold + soft, n) * smoothstep(0.0, 0.1, amount) * density
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The share of points a layer covers follows the share it is given: a thin layer leaves
    /// most of the ground bare, a full one covers most of it, and density thins it within.
    #[test]
    fn a_layer_covers_about_its_share() {
        let covered = |amount: f32| {
            let mut sum = 0.0;
            for i in 0..4000u16 {
                let p = Vec3::new(f32::from(i % 63) * 0.37, 1.3, f32::from(i / 63) * 0.41);
                sum += layer_mask(p, 1, (1.4, 1.0), amount);
            }
            sum / 4000.0
        };
        let (thin, half, full) = (covered(0.15), covered(0.5), covered(0.9));
        assert!(thin < 0.2 && full > 0.8, "{thin} {half} {full}");
        assert!((0.3..0.7).contains(&half), "{half}");
        assert!(layer_mask(Vec3::ONE, 0, (1.0, 1.0), 0.0).abs() < 1e-9);
    }

    /// Continuous across the fine period's wrap and the pattern's: the noise is periodic.
    #[test]
    fn the_noise_is_periodic() {
        let a = noise_at(Vec3::new(0.3, 2.0, 5.0), 0.25, 3.0);
        let b = noise_at(Vec3::new(8.3, 2.0, 5.0), 0.25, 3.0);
        assert!((a - b).abs() < 1e-5, "{a} {b}");
        let c = noise_at(Vec3::new(1.0, 7.0, 3.0), 5.0, 1.0);
        let d = noise_at(Vec3::new(1025.0, 7.0, 3.0), 5.0, 1.0);
        assert!((c - d).abs() < 1e-4, "{c} {d}");
    }
}
