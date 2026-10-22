//! Reach (G1.4d, ADR-0027 decision 4): what a character's hands can act on, the same for the
//! client that aims and the server that checks. A body's reach rounded up (Minecraft lets a
//! player act about 4.5 blocks from the eye): 2.5 m from the hands, the tool's length within
//! it. The body never turns (its heading is the animation's), so the hands are measured from
//! where they hang, on the body's axis: their centre between the two hands standing.

use crate::character::{self, bones};
use creeer_math::glam::DVec3;

/// How far from the hands a character acts (m).
pub const REACH: f64 = 2.5;
/// How long the server remembers where bodies were (ticks): what a client saw may be this old
/// at most (Source's lag compensation keeps one second).
pub const HISTORY: u32 = 60;

/// How far above the body's centre the hands hang, and the eyes see from (m), standing.
fn heights() -> (f64, f64) {
    let centres = bones::centres();
    // The model stands on its feet at the origin, +y up; the body's centre is above them.
    let centre = character::heights().0;
    let hands = 0.5 * (centres[5].y + centres[11].y) - centre;
    (hands, centres[2].y - centre)
}

/// Where a character's hands are measured from: its body's centre at `centre`, upright along
/// `up`.
pub fn hands(centre: DVec3, up: DVec3) -> DVec3 {
    centre + up.normalize() * heights().0
}

/// Where a character sees from (its head's centre).
pub fn eye(centre: DVec3, up: DVec3) -> DVec3 {
    centre + up.normalize() * heights().1
}

/// Whether `point` is within reach of a character whose body's centre is at `centre`.
pub fn within(centre: DVec3, up: DVec3, point: DVec3) -> bool {
    (point - hands(centre, up)).length() <= REACH
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hands_hang_at_the_hips_and_the_eyes_are_in_the_head() {
        let up = DVec3::Y;
        let feet = DVec3::ZERO;
        let centre = feet + up * character::heights().0;
        // People's knuckles hang about 0.75 m above the floor, their eyes about 1.6 m (the
        // engineer stands about 1.75 m tall).
        let (hands, eye) = (hands(centre, up).y, eye(centre, up).y);
        assert!((0.65..0.9).contains(&hands), "the hands {hands} m up");
        assert!((1.5..1.75).contains(&eye), "the eyes {eye} m up");
    }

    #[test]
    fn a_point_is_within_reach_up_to_two_and_a_half_metres_from_the_hands() {
        let (centre, up) = (DVec3::new(0.0, 1.0, 0.0), DVec3::Y);
        let h = hands(centre, up);
        assert!(within(centre, up, h + DVec3::X * 2.49));
        assert!(!within(centre, up, h + DVec3::X * 2.51));
        // The ground two metres ahead of the feet, and a shelf above the head, are in reach.
        let feet = centre - up * character::heights().0;
        assert!(within(centre, up, feet + DVec3::Z * 2.0));
        assert!(within(centre, up, feet + up * 2.4));
    }
}
