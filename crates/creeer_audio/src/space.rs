//! Where a sound is heard from (ADR-0037, decision 4): its distance (pressure as 1/r), the air
//! between (ISO 9613-1: high frequencies absorbed with distance, as a low-pass whose corner falls
//! as 1/√r), its direction (equal-power stereo), and its travel at the speed of sound.

use crate::acoustic::SOUND_SPEED;
use creeer_math::glam::DVec3;

/// The listener: where the ears are, and their right.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Listener {
    pub at: DVec3,
    pub right: DVec3,
}

impl Default for Listener {
    fn default() -> Self {
        Self {
            at: DVec3::ZERO,
            right: DVec3::X,
        }
    }
}

/// How a sound reaches the ears: each ear's gain (per Pa at a metre), the air's low-pass corner
/// (Hz) and the distance (m).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub gains: [f32; 2],
    pub air: f32,
    pub distance: f64,
}

/// Nearer than this a sound is as loud as here (m): a source is not a point.
const NEAREST: f64 = 0.5;
/// Air's absorption, taken as k·f² dB a metre: 1.3e-9 gives 0.02 dB/m at 4 kHz and 0.08 at
/// 8 kHz (ISO 9613-1 at 20 °C and 50–70 % humidity).
const ABSORPTION: f64 = 1.3e-9;

/// The corner (Hz) of the low-pass that stands for `distance` m of air: where it absorbs 3 dB.
pub fn air_corner(distance: f64) -> f32 {
    #[allow(clippy::cast_possible_truncation)] // Hz
    let f = (3.0 / (ABSORPTION * distance.max(1e-3))).sqrt() as f32;
    f.clamp(300.0, 20_000.0)
}

/// How a sound at `at` reaches `listener`.
pub fn place(listener: &Listener, at: DVec3) -> Placement {
    let to = at - listener.at;
    let distance = to.length();
    let side = if distance > 1e-6 {
        to.dot(listener.right) / distance
    } else {
        0.0
    };
    // Equal power, each ear at 1 for a sound ahead, the far ear never below a third.
    let theta = std::f64::consts::FRAC_PI_4 * (1.0 + 0.7 * side.clamp(-1.0, 1.0));
    let near = 1.0 / distance.max(NEAREST);
    #[allow(clippy::cast_possible_truncation)] // gains
    let gains = [
        (std::f64::consts::SQRT_2 * theta.cos() * near) as f32,
        (std::f64::consts::SQRT_2 * theta.sin() * near) as f32,
    ];
    Placement {
        gains,
        air: air_corner(distance),
        distance,
    }
}

/// Samples a sound takes to travel `distance` m at `rate` samples a second.
pub fn travel(distance: f64, rate: f32) -> u32 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a few seconds at most
    let samples = (distance / SOUND_SPEED * f64::from(rate)).round() as u32;
    samples
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn far_is_quieter_duller_and_later_and_the_right_is_to_the_right() {
        let ears = Listener::default();
        let near = place(&ears, DVec3::new(0.0, 0.0, 2.0));
        let far = place(&ears, DVec3::new(0.0, 0.0, 20.0));
        assert!((near.gains[0] / far.gains[0] - 10.0).abs() < 1e-4, "1/r");
        assert!(
            (near.gains[0] - near.gains[1]).abs() < 1e-6,
            "ahead: both ears alike"
        );
        assert!(far.air < near.air);
        // At 100 m the air's corner is near 5 kHz (8 kHz loses 8 dB there).
        assert!((4000.0..6000.0).contains(&air_corner(100.0)));
        let right = place(&ears, DVec3::new(5.0, 0.0, 0.0));
        assert!(right.gains[1] > 3.0 * right.gains[0]);
        // 343 m: a second.
        assert_eq!(travel(SOUND_SPEED, 48_000.0), 48_000);
    }
}
