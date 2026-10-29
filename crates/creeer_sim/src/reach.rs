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

/// How far above the body's centre the hands hang, the eyes see from and the shoulders turn
/// (m), standing.
fn heights() -> (f64, f64, f64) {
    let centres = bones::centres();
    // The model stands on its feet at the origin, +y up; the body's centre is above them.
    let centre = character::heights().0;
    let hands = 0.5 * (centres[5].y + centres[11].y) - centre;
    // The joints at the upper arms' heads.
    let joints = bones::joints();
    let shoulder = |arm: usize| joints.iter().find(|j| j.0 == arm).map_or(0.0, |j| j.2.y);
    let shoulders = 0.5 * (shoulder(3) + shoulder(9)) - centre;
    (hands, centres[2].y - centre, shoulders)
}

/// Where a character's hands are measured from: its body's centre at `centre`, upright along
/// `up`.
pub fn hands(centre: DVec3, up: DVec3) -> DVec3 {
    centre + up.normalize() * heights().0
}

/// Where a character's arms turn from: their shoulders' centre (the swing's pivot).
pub fn shoulders(centre: DVec3, up: DVec3) -> DVec3 {
    centre + up.normalize() * heights().2
}

/// How far the shoulders come down and out towards what the hands reach below them, stooping
/// and squatting (m; G2.10, ADR-0036): from 0.818 of the stature standing to about 0.43 of it
/// squatting, the trunk leant forward (Winter's segments: a 1.75 m person's shoulders 0.7 m
/// lower, 0.35 m on), so the hands reach down to the ground just before the feet.
pub const STOOP_DOWN: f64 = 0.7;
pub const STOOP_OUT: f64 = 0.35;

/// The `shoulders` (standing, upright along `up`) as near `target` as stooping and squatting
/// bring them: down to it by [`STOOP_DOWN`] at most, and out towards it as they come down. The
/// server's hands reach from there (`creeer_server`'s `hands.rs`), and the drawn body stoops so.
pub fn stooped(shoulders: DVec3, up: DVec3, target: DVec3) -> DVec3 {
    let off = target - shoulders;
    let down = (-off.dot(up)).clamp(0.0, STOOP_DOWN);
    let out = off.reject_from(up);
    let on = out.length().min(STOOP_OUT * down / STOOP_DOWN);
    shoulders - up * down + out.normalize_or_zero() * on
}

/// The farthest what the hands hold is from the shoulders (m: forward grip reach about 0.7 m;
/// G2.10, ADR-0036).
pub const ARM: f64 = 0.75;

/// `target` brought within the arms' reach of the shoulders as they stoop to it ([`stooped`]),
/// for a body at `centre` upright along `up`: where the hands can hold a point meant there. The
/// server's hands hold there (`hands.rs`), and a client steers there (G2.10: a hold meant
/// beyond it was drawn where no hand reached, the arm through the body).
pub fn within_arms(centre: DVec3, up: DVec3, target: DVec3) -> DVec3 {
    let shoulders = stooped(shoulders(centre, up), up, target);
    let off = target - shoulders;
    if off.length() > ARM {
        shoulders + off * (ARM / off.length())
    } else {
        target
    }
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
        // Their shoulders, about 1.45 m.
        let shoulders = shoulders(centre, up).y;
        assert!(
            (1.35..1.55).contains(&shoulders),
            "the shoulders {shoulders} m up"
        );
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
