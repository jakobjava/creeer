//! Swimming (G1.2b, ADR-0023): the stroke and the swimmer's balance.

use super::{BALANCE, MASS, RESPONSE, SWIM_BALANCE, SWIM_MOST};
use creeer_math::glam::{DQuat, DVec3};

/// A swimmer's force and torque for a step: towards the velocity asked for (`walk`), up while
/// asked to rise (`jump`), within a swimmer's thrust; upright, more weakly than on its feet.
/// The body upright along `up`, turned `q`, moving at `v` and spinning at `w`. The sea's
/// buoyancy and drag are its own (`surroundings.rs`).
pub(super) fn stroke(
    (walk, jump): (DVec3, bool),
    (up, q, v, w): (DVec3, DQuat, DVec3, DVec3),
) -> (DVec3, DVec3) {
    let along = |u: DVec3| u - up * u.dot(up);
    let mut stroke = (along(walk) - along(v)) * (MASS / RESPONSE.0);
    if jump {
        stroke += up * SWIM_MOST;
    }
    if stroke.length() > SWIM_MOST {
        stroke = stroke.normalize() * SWIM_MOST;
    }
    let tilt = (q * DVec3::Y).cross(up);
    let (k, c, _) = BALANCE;
    let mut torque = tilt * k - w * c;
    if torque.length() > SWIM_BALANCE {
        torque = torque.normalize() * SWIM_BALANCE;
    }
    (stroke, torque)
}
