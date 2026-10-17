//! Snapshot interpolation on the client: entities drawn a little in the past, between the
//! two snapshots around that moment, so they move smoothly however the packets arrive.
//!
//! Positions follow the cubic Hermite curve through both states with their velocities (a
//! falling crate keeps its parabola between snapshots 33 ms apart), orientations the
//! shortest arc. Past the newest snapshot an entity runs on at its velocity for at most
//! [`EXTRAPOLATE`] ticks, then waits.

use crate::entity::EntityId;
use crate::quant::BodyState;
use crate::snapshot::Receiver;
use creeer_math::glam::{DQuat, DVec3};
use std::collections::BTreeMap;

/// How far past the newest snapshot an entity may be run on (ticks: 100 ms at 60 a second).
pub const EXTRAPOLATE: f64 = 6.0;

/// Where an entity is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    /// Position (m).
    pub position: DVec3,
    /// Orientation.
    pub orientation: DQuat,
}

/// Every entity's pose at server tick `tick` (fractional), `dt` seconds a tick.
pub fn sample(receiver: &Receiver, tick: f64, dt: f64) -> BTreeMap<EntityId, Pose> {
    let snapshots: Vec<_> = receiver.snapshots().collect();
    let Some(newest) = snapshots.last() else {
        return BTreeMap::new();
    };
    let after = snapshots.iter().position(|s| f64::from(s.tick) > tick);
    match after {
        // Before the oldest: the oldest as it is.
        Some(0) => run_on(snapshots[0].states.iter(), 0.0),
        // Between two.
        Some(k) => {
            let (a, b) = (snapshots[k - 1], snapshots[k]);
            let span = f64::from(b.tick - a.tick);
            let u = (tick - f64::from(a.tick)) / span;
            b.states
                .iter()
                .map(|(id, sb)| {
                    let pose = match a.states.get(id) {
                        Some(sa) => between(sa, sb, u, span * dt),
                        None => pose(sb),
                    };
                    (*id, pose)
                })
                .collect()
        }
        // After the newest: run on.
        None => {
            let ahead = (tick - f64::from(newest.tick)).clamp(0.0, EXTRAPOLATE);
            run_on(newest.states.iter(), ahead * dt)
        }
    }
}

/// The states run on for `ahead` seconds at their velocities (those asleep stay put).
fn run_on<'a>(
    states: impl Iterator<Item = (&'a EntityId, &'a BodyState)>,
    ahead: f64,
) -> BTreeMap<EntityId, Pose> {
    states
        .map(|(id, s)| {
            let mut p = pose(s);
            if !s.asleep && ahead > 0.0 {
                p.position += s.velocity() * ahead;
                p.orientation =
                    (DQuat::from_scaled_axis(s.spin() * ahead) * p.orientation).normalize();
            }
            (*id, p)
        })
        .collect()
}

fn pose(s: &BodyState) -> Pose {
    Pose {
        position: s.position(),
        orientation: s.orientation(),
    }
}

/// The Hermite blend of `a` and `b` at `u` in [0, 1], `span` seconds apart.
fn between(a: &BodyState, b: &BodyState, u: f64, span: f64) -> Pose {
    let (p0, p1) = (a.position(), b.position());
    let (v0, v1) = (a.velocity() * span, b.velocity() * span);
    let (u2, u3) = (u * u, u * u * u);
    let position = p0 * (2.0 * u3 - 3.0 * u2 + 1.0)
        + v0 * (u3 - 2.0 * u2 + u)
        + p1 * (-2.0 * u3 + 3.0 * u2)
        + v1 * (u3 - u2);
    Pose {
        position,
        orientation: a.orientation().slerp(b.orientation(), u),
    }
}

/// The server tick to draw at: a little behind the newest tick the client thinks the
/// server is at, so a snapshot or two may be late or lost and still be waited for.
#[derive(Clone, Copy, Debug)]
pub struct Playback {
    /// Ticks behind the server.
    pub delay: f64,
    /// The server tick estimated now.
    estimate: Option<f64>,
}

impl Playback {
    /// Drawing `delay` ticks behind the server (two snapshots at 30 a second: 4 ticks).
    pub fn new(delay: f64) -> Self {
        Self {
            delay,
            estimate: None,
        }
    }

    /// Advances the estimate by `ticks` of local time and pulls it a tenth of the way
    /// towards the newest snapshot's tick (the server's clock, as the packets tell it):
    /// smooth however they arrive, and never running away. Returns the tick to draw at.
    pub fn advance(&mut self, ticks: f64, newest: Option<u32>) -> f64 {
        let Some(newest) = newest.map(f64::from) else {
            return 0.0;
        };
        let estimate = match self.estimate {
            None => newest,
            Some(e) => {
                let e = e + ticks;
                // A long stall or a jump: start again from the packets.
                if (e - newest).abs() > 30.0 {
                    newest
                } else {
                    e + 0.1 * (newest - e)
                }
            }
        };
        self.estimate = Some(estimate);
        estimate - self.delay
    }
}

#[cfg(test)]
mod tests;
