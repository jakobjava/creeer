//! Stopping and standing still (G1.4a; G1.10): a stop finishes the step in hand and stands
//! where it set the feet (people stop in a little more than a step: Ridge et al. 2013), and a
//! person standing steps into place only when a foot is well off where it would stand (the body
//! turned on the spot, or pushed), once the body is at rest. (The user, 2026-10-23: it took
//! steps on the spot after stopping, the arms swinging with them.)

use super::feet::Foot;
use creeer_math::glam::DVec3;

/// A foot this far from where it would stand still steps there (m): about half a boot's
/// length, an offset that shows; nearer, the person stands as the feet came to rest.
pub(super) const SETTLE: f64 = 0.12;
/// Stood still, a foot steps into place only this far off where it would stand (m).
const STRAY: f64 = 0.15;
/// Slower than this (m/s) the body has come to rest after a stop: its last drift is no reason
/// to step.
const AT_REST: f64 = 0.03;

/// Whether a foot stands where it would stand still (`rest`: the ground's point and normal).
fn placed((f, (g, _)): (&Foot, &(DVec3, DVec3))) -> bool {
    (f.plant - *g).length() < SETTLE
}

/// Whether the feet stand where they would stand still, both down.
pub(super) fn settled(feet: &[Foot; 2], rest: &[(DVec3, DVec3); 2]) -> bool {
    feet.iter().all(|f| f.down) && feet.iter().zip(rest).all(placed)
}

/// Whether a foot, the body at rest at `speed` (m/s), is so far off (or not down) that it steps.
pub(super) fn strayed(feet: &[Foot; 2], rest: &[(DVec3, DVec3); 2], speed: f64) -> bool {
    speed < AT_REST
        && (feet.iter().zip(rest)).any(|(f, (g, _))| !f.down || (f.plant - *g).length() > STRAY)
}

/// Stopping: notes each foot that has been in the air since the stop began (`up`).
pub(super) fn track(up: &mut [bool; 2], feet: &[Foot; 2]) {
    for (up, f) in up.iter_mut().zip(feet) {
        *up |= !f.down;
    }
}

/// Whether a stop is over: both feet down, and a foot in the air since it began has landed (the
/// step in hand finished) with neither left far off (else the other closes up in the same stop,
/// as people take a little more than a step: standing a frame between, it flickered), or both
/// already stand in place.
pub(super) fn over(feet: &[Foot; 2], rest: &[(DVec3, DVec3); 2], up: [bool; 2]) -> bool {
    let landed = (0..2).any(|s| feet[s].down && up[s]);
    let near = (feet.iter().zip(rest)).all(|(f, (g, _))| (f.plant - *g).length() <= STRAY);
    feet.iter().all(|f| f.down) && ((landed && near) || settled(feet, rest))
}
