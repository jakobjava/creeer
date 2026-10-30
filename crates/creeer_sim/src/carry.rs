//! Carrying and throwing (G1.4f, ADR-0027 decision 6): how much a person holds up and how fast
//! they throw.
//!
//! Held: the most a man lifts to elbow height, 58.6 kg on average (McDaniel et al., 1,066 U.S.
//! Air Force recruits lifting an increasing weight until they could not; SD 11.2 kg), as
//! Gallagher et al. report it for NIOSH. A load is held in both hands at the elbows' height,
//! the forearms forward.
//!
//! Thrown: an overarm throw gives the object and the forearm and hand that throw it about the
//! same kinetic energy whatever its mass, the object's share m/(m + B), B a third of the forearm
//! and hand's mass (Cross 2004, five men throwing 57 g to 3.4 kg: a factor of 60 in mass, of
//! only 2.4 in speed). Its energy A was 94–179 J, 128 J on average.

use crate::character::{self, bones};
use creeer_math::glam::{DQuat, DVec3};

/// Standard gravity (m/s²).
const G: f64 = 9.806_65;
/// The most a man lifts to elbow height (kg), and so the most force his hands hold with (N).
pub const MOST_LIFTED: f64 = 58.6;
pub const STRENGTH: f64 = MOST_LIFTED * G;
/// A throw's energy, shared by the object and the forearm and hand (J), and the forearm and
/// hand's share of a man's mass (2.52 %: published segment masses, as Cross takes them).
const THROW: f64 = 128.0;
const FOREARM_AND_HAND: f64 = 0.0252;
/// A man's mass (kg): the engineer without his suit and pack.
const BODY: f64 = 80.0;
/// Both arms' mass (kg): upper arm, forearm and hand, 4.94 % of a man's each (de Leva 1996).
/// The hands' strength moves them too, so even a pebble is lifted no faster than an arm moves.
pub const ARMS: f64 = 2.0 * 0.0494 * BODY;

/// The two hands this far apart on what they hold (m), each holding at most half the strength
/// (G2.10, ADR-0036: their torque on it, and where they are drawn on a pole).
pub const HANDS_APART: f64 = 0.4;
/// Below this share of its largest moment a load's least is about its own long axis: a pole,
/// held and drawn along that axis (G2.10).
pub const SLENDER: f64 = 0.1;

/// The hands' plan of where they bring what they hold (G2.10, ADR-0036; `creeer_server`'s
/// `hands.rs`, and the client drawing its own load ahead of it): critically damped at this
/// frequency (Hz), at most this fast (m/s: peak hand speeds in reaching, 1.5–2 m/s) and turning
/// it at most this fast (rad/s).
pub const PLAN_HZ: f64 = 3.0;
pub const FASTEST: f64 = 2.0;
pub const TURNING_AT_MOST: f64 = 6.0;
/// How fast the hands turn a thing the player aims (rad/s: a flick of the wrists; G2.10, the
/// first-person placing, `creeer_server`'s `hands.rs` and the client drawing its own load): it
/// moves at most [`FASTEST`] (as fast as hands reach; at 4 m/s a post driven into its hole
/// struck its floor and was thrown).
pub const AIMED_TURN: f64 = 8.0;

/// One step of `dt` s of the hands bringing a thing the player aims from `(at, turn)` to where
/// it is aimed, `(to, to_turn)`: straight there, at most [`FASTEST`] and [`AIMED_TURN`]. Returns
/// where and how it is turned then, its velocity and its spin. The server's hands and the client
/// drawing its own load step by this one law, so the drawn load goes where the server's will.
pub fn aimed_step(
    (at, turn): (DVec3, DQuat),
    (to, to_turn): (DVec3, DQuat),
    dt: f64,
) -> (DVec3, DQuat, DVec3, DVec3) {
    let velocity = ((to - at) / dt).clamp_length_max(FASTEST);
    let spin = ((to_turn * turn.conjugate()).normalize().to_scaled_axis() / dt)
        .clamp_length_max(AIMED_TURN);
    let turned = (DQuat::from_scaled_axis(spin * dt) * turn).normalize();
    (at + velocity * dt, turned, velocity, spin)
}

/// The most the hands' plan accelerates a load of `mass` (kg) in one hand or two, under gravity
/// `g` (m/s²): half what their strength leaves over its weight gives it and the arms
/// ([`spare`]'s law), at least 0.5 m/s² (a load too heavy to lift still slides along).
pub fn plan_spare(mass: f64, g: f64, one_hand: bool) -> f64 {
    let strength = if one_hand { 0.5 } else { 1.0 } * STRENGTH;
    (0.5 * (strength - mass * g) / (mass + ARMS)).max(0.5)
}

/// The acceleration the hands lift a load of `mass` (kg) with: half what their strength leaves
/// over its weight gives it and the arms (m/s²; not positive: too heavy to lift).
pub fn spare(mass: f64) -> f64 {
    0.5 * (STRENGTH - mass * G) / (mass + ARMS)
}

/// Where a load is held, from the body's centre at `centre`, upright along `up`, facing
/// `facing` (horizontal): at the elbows' height, a forearm's length ahead of them.
pub fn held(centre: DVec3, up: DVec3, facing: DVec3) -> DVec3 {
    held_out(centre, up, facing, hold().1)
}

/// Where a load is held `ahead` m before the body's axis (farther than a forearm, the arms
/// reaching out round a load that would not otherwise clear the body).
pub fn held_out(centre: DVec3, up: DVec3, facing: DVec3, ahead: f64) -> DVec3 {
    centre + up.normalize() * hold().0 + facing.normalize_or_zero() * ahead
}

/// Where the right hand holds a tool at the body's side (a tool is carried in one hand, as the
/// catalogue's are drawn: the hand hanging as the skeleton stands), from the body's centre at
/// `centre`, upright along `up`, facing `facing` (horizontal).
pub fn at_side(centre: DVec3, up: DVec3, facing: DVec3) -> DVec3 {
    let z = facing
        .reject_from(up)
        .normalize_or(up.any_orthonormal_vector());
    let y = up.normalize();
    let x = y.cross(z);
    // The right hand's centre standing, from the body's centre (the model's frame: y up, z
    // forward).
    let hand = bones::centres()[5] - DVec3::Y * character::heights().0;
    centre + x * hand.x + y * hand.y + z * hand.z
}

/// How far before the body's axis the hands hold a load at least (m): a forearm.
pub fn forearm() -> f64 {
    hold().1
}

/// How far the forearms are raised from level carrying (rad): things are carried before the
/// chest (the user, 2026-10-29: "things are usually carried in front of the chest and not at
/// some random position close to the body"), held against it, the upper arms hanging.
const FOREARMS_RAISED: f64 = 30.0 * std::f64::consts::PI / 180.0;

/// How far above the body's centre the hands hold what they carry, and how far ahead of the
/// body's axis (m), from the skeleton standing: the elbows at the body's sides, the forearms
/// raised [`FOREARMS_RAISED`] from level.
fn hold() -> (f64, f64) {
    let (elbow, forearm) = elbows();
    let (up, ahead) = creeer_math::det::sin_cos(FOREARMS_RAISED);
    (elbow + forearm * up, forearm * ahead)
}

/// How far above the body's centre the elbows are, and the forearm's length to the hand's
/// centre (m), from the skeleton standing.
fn elbows() -> (f64, f64) {
    let joints = bones::joints();
    let at = |child: usize| {
        joints
            .iter()
            .find(|j| j.0 == child)
            .map_or(DVec3::ZERO, |j| j.2)
    };
    let centres = bones::centres();
    // The elbows (the forearms' joints) and the forearm from the elbow to the hand's centre.
    let elbow = 0.5 * (at(4) + at(10));
    // The elbows at the body's sides, the forearms swung forward level.
    let forearm = (centres[5] - at(4)).length();
    (elbow.y - character::heights().0, forearm)
}

/// How fast a man throws an object of `mass` (kg) overarm (m/s).
pub fn throw_speed(mass: f64) -> f64 {
    let b = FOREARM_AND_HAND * BODY / 3.0;
    (2.0 * THROW / (mass + b)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tennis_ball_and_a_lead_brick_fly_as_cross_measured() {
        // His throwers: the tennis ball at about 20 m/s, the 3.4 kg brick at about 8.5 (Fig. 5:
        // 19.5 and 8.5 m/s), a factor of 2.4 apart over the whole range.
        let (ball, brick) = (throw_speed(0.057), throw_speed(3.4));
        assert!((17.0..23.0).contains(&ball), "{ball} m/s");
        assert!((7.0..10.0).contains(&brick), "{brick} m/s");
        assert!((ball / brick - 2.4).abs() < 0.4, "{} times", ball / brick);
        // A 20 kg box: a toss.
        assert!(throw_speed(20.0) < 4.0);
    }

    #[test]
    fn a_load_is_carried_before_the_chest() {
        let (centre, up) = (DVec3::new(0.0, 1.0, 0.0), DVec3::Y);
        let feet = centre - up * character::heights().0;
        let p = held(centre, up, DVec3::Z);
        // Before the lower chest (the user, 2026-10-29): the breastbone's lower end is about 0.72
        // of the stature up, 1.33 m on the 1.85 m engineer (elbows about 1.1 m); a forearm, raised
        // 30°, ahead of the elbows.
        let (height, ahead) = ((p - feet).y, p.z);
        assert!((1.25..1.4).contains(&height), "{height} m up");
        assert!((0.15..0.35).contains(&ahead), "{ahead} m ahead");
        // The hands hold up what a man lifts to his elbows.
        assert!((STRENGTH - 574.7).abs() < 0.1);
    }
}
