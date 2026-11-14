//! The legs (G1.2a, ADR-0023; how they meet what they step onto, G2.11, ADR-0042): the spring
//! and damper they push the body up with, and the jump they push off with.
//!
//! **How fast the legs straighten onto a body.** The legs are one ray down from the hips: a deck,
//! a log or a crate stepped onto is found under them at once, and they find themselves pressed by
//! its whole height. On such a body (at least [`STEPPED_ON`]) they press it at most [`RISING`]
//! beyond the weight's compression, so the body rises at about k·`RISING`/c, 0.5 m/s, as a
//! person's centre rises stepping up (stair ascent's, about 0.3–0.5 m/s), and a deck stepped onto
//! is pressed with about 1.5 body weights, as people's footfalls press (1.1–1.5). (Pushing the
//! legs' whole 3,600 N, 2.8 body weights in a tick, the first person stepping onto *bridge_demo*'s
//! nailed deck pulled its nails out and broke it, G2.11.) A body coming down onto them (a
//! landing) is met as far as they are pressed; so are a light body (a pole is pressed into what
//! is under it) and the fixed ground, as ADR-0023 has them, bit for bit.

use super::{JUMP_SPEED, LANDED, LEG_DAMPING, LEG_LENGTH, LEG_STIFFNESS, MASS};
use creeer_math::glam::DVec3;
use creeer_physics::world::World;
use creeer_physics::world::ray::Hit;

/// How much further than its weight presses them the legs push a body they find suddenly under
/// them (m): the body then rises at k·`RISING`/c, 0.49 m/s.
const RISING: f64 = 0.035;
/// A body coming down onto its legs at v (m/s) is met as far as they are pressed, up to v times
/// this (s) beyond the weight's compression and [`RISING`].
const LANDING: f64 = 0.5;
/// What the legs step up onto at a person's pace weighs at least this (kg), half a person: a
/// deck, a plank bridge with its sills, a big log, a crate. On a lighter body (a pole, a box, a
/// ragdoll's limb) they press as far as they are pressed, as before: a person's weight presses
/// such a body into what is under it. (Capped on a 2.5 kg pole too, *builders*' third player
/// standing on its crossed poles at the start took 1,504 Newton iterations a tick against 780,
/// and its clients replayed a third more; at 20 kg, stream B's 27 kg poles laid across a gap
/// were stood on and turned off the way across.)
const STEPPED_ON: f64 = 50.0;

/// Whether the legs stand on `hit` on a body they step onto ([`STEPPED_ON`] or heavier).
pub(super) fn stepped_onto(world: &World, hit: &Hit) -> bool {
    hit.body
        .is_some_and(|(b, _)| world.bodies[b].structure.masses().iter().sum::<f64>() >= STEPPED_ON)
}

/// `v` at most `most` long (as the walk has always cut it: its own direction times the most,
/// bit for bit).
pub(super) fn most(v: DVec3, most: f64) -> DVec3 {
    if v.length() > most {
        v.normalize() * most
    } else {
        v
    }
}

/// A jump asked of character `index` standing on `hit`, at `x` moving `v` over ground moving
/// `ground_v`, upright along `up`: it leaves at the jump's speed once landed (held, the key
/// jumped again at the legs' first touch, still falling: hops), and pushes what it stood on
/// back.
pub(super) fn jump(
    world: &mut World,
    (index, hit): (usize, &Hit),
    (x, v, ground_v, up): (DVec3, DVec3, DVec3, DVec3),
) {
    let spring = LEG_STIFFNESS * (LEG_LENGTH - hit.distance);
    let landed = spring >= LANDED.0 && (v - ground_v).dot(up) > -LANDED.1;
    if v.dot(up) < 0.5 * JUMP_SPEED && landed {
        let impulse = up * (MASS * (JUMP_SPEED - v.dot(up).max(0.0)));
        world.apply_impulse(index, 0, impulse, x);
        if let Some((other, node)) = hit.body {
            world.apply_impulse(other, node, -impulse, hit.point);
        }
    }
}

/// How far the body's weight presses the legs standing (m).
fn standing() -> f64 {
    MASS * 9.806_65 / LEG_STIFFNESS
}

/// The legs' rest length, shortened by what they are pressed beyond what they push a body
/// stepped onto with (`capped`), for legs `distance` from the hips to the ground, the body rising
/// at `rise` (m/s) over what is under it: their rest (as the hips' distance to the ground) and
/// the force they push with as the step begins (N). Otherwise ADR-0023's law exactly.
pub(super) fn rest(distance: f64, rise: f64, capped: bool) -> (f64, f64) {
    let pressed = LEG_LENGTH - distance;
    let allowed = standing() + RISING + LANDING * (-rise).max(0.0);
    let excess = if capped {
        (pressed - allowed).max(0.0)
    } else {
        0.0
    };
    let push = LEG_STIFFNESS * (pressed - excess) - LEG_DAMPING * rise;
    (LEG_LENGTH - excess, push)
}
