//! Quantised states keep the precision promised, quantise to themselves, and cost little
//! when they change little.

use super::*;
use creeer_math::codec::{Reader, Writer};

/// Deterministic numbers in [0, 1).
fn numbers(seed: u64) -> impl FnMut() -> f64 {
    let mut x = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
    move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        #[allow(clippy::cast_precision_loss)] // 53 bits
        let v = (x >> 11) as f64 / (1u64 << 53) as f64;
        v
    }
}

#[test]
fn positions_are_kept_to_half_a_step_on_a_planet() {
    let mut next = numbers(1);
    for _ in 0..10_000 {
        let p = DVec3::new(next() - 0.5, next() - 0.5, next() - 0.5) * 1.2e6;
        let q = Quantised::new(p, POSITION_SCALE);
        let back = q.value(POSITION_SCALE);
        assert!((back - p).abs().max_element() <= 0.5 / POSITION_SCALE + 1e-9);
        assert_eq!(Quantised::new(back, POSITION_SCALE), q, "idempotent");
    }
}

#[test]
fn rotations_are_kept_to_a_tenth_of_a_milliradian() {
    let mut next = numbers(2);
    let mut worst = 0.0f64;
    for _ in 0..10_000 {
        let axis = DVec3::new(next() - 0.5, next() - 0.5, next() - 0.5).normalize();
        let q = DQuat::from_axis_angle(axis, 7.0 * next());
        let r = Rotation::new(q);
        let back = r.value();
        worst = worst.max(back.angle_between(q));
        assert_eq!(Rotation::new(back), r, "idempotent");
        // Both signs of the same rotation quantise alike.
        assert_eq!(Rotation::new(-q), r);
    }
    assert!(worst < 1e-4, "worst {worst} rad");
}

#[test]
fn a_body_that_moved_little_costs_a_few_bytes() {
    let base = Quantised::new(DVec3::new(250_123.4, -10.0, 433_000.25), POSITION_SCALE);
    let moved = Quantised::new(DVec3::new(250_123.41, -10.0, 433_000.26), POSITION_SCALE);
    let mut w = Writer::new();
    moved.write(&mut w, base);
    let bytes = w.finish();
    assert!(bytes.len() <= 4, "{} bytes", bytes.len());
    assert_eq!(Quantised::read(&mut Reader::new(&bytes), base), Ok(moved));
    let mut w = Writer::new();
    let r = Rotation::new(DQuat::from_rotation_z(1.0));
    r.write(&mut w);
    let bytes = w.finish();
    assert_eq!(bytes.len(), 7);
    assert_eq!(Rotation::read(&mut Reader::new(&bytes)), Ok(r));
    assert!(Rotation::read(&mut Reader::new(&[0xff; 7])).is_err());
}
