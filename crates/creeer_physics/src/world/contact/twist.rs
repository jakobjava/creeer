//! Drilling friction. A ball pressed into a surface touches it over a small circle, Hertz's
//! of radius a = √(R·δ), and friction over that circle resists its spinning about the
//! normal: sliding, with the torque μ·N·3πa/16 of Hertz's pressure (Lubkin, 1951). A point
//! contact's friction has no lever about its own normal: without this a ball, or an avatar,
//! left spinning about the vertical spun on for good (F6).
//!
//! Its energy is that of the torque over the twist φ of A relative to B about the normal since
//! the step's start, with the lagged normal force: E = μ·λ·r·f₀(φ), r = 3πa/16, f₀ the
//! smoothing of sliding friction (`friction.rs`) over the twist a spin of [`SPIN_SLIP`] makes
//! in the step. Below that spin it sticks (a sliding contact's 0.1 mm, measured at the
//! circle's lever of a millimetre, let a ball spin on for a minute); and a spin held by less
//! than the full torque creeps at most that fast (sliding friction keeps its anchor across
//! steps; this keeps none).

use super::{Contacts, Law, Side};
use crate::structure::group::MemberState;
use creeer_math::det::{atan2, cbrt};
use creeer_math::glam::{DQuat, DVec3};

/// The spin below which drilling friction sticks (rad/s).
pub const SPIN_SLIP: f64 = 1e-3;

/// The smoothing f₀ of sliding friction (`Contacts::smooth`) over `e`, with f₀′(x)/x and f₀″.
fn smooth(x: f64, e: f64) -> (f64, f64, f64) {
    if x >= e {
        (x, 1.0 / x, 0.0)
    } else {
        (
            -x * x * x / (3.0 * e * e) + x * x / e + e / 3.0,
            2.0 / e - x / (e * e),
            2.0 / e - 2.0 * x / (e * e),
        )
    }
}

impl Contacts {
    /// Contact `i`'s lever of drilling friction (m): 3π/16 of Hertz's contact radius at its
    /// lagged force; zero but for a ball's contact between rigid bodies (a structure's nodes
    /// take no turning stiffness from contacts into their matrices, and a bridge resting on
    /// its joints took a fifth more iterations a step).
    fn twist_lever(&self, i: usize) -> f64 {
        let c = &self.list[i];
        match c.law {
            Law::Hertz(h) if c.drills && c.lambda > 0.0 => {
                3.0 * std::f64::consts::PI / 16.0 * h.radius.sqrt() * cbrt(c.lambda / h.k)
            }
            _ => 0.0,
        }
    }

    /// The twist of contact `i`'s side A relative to B about its normal since the step's
    /// start (rad), and its slopes by A's turn and by B's (each a turn δθ of the side's node,
    /// exp(δθ)·q).
    fn twist(&self, i: usize, states: &[MemberState<'_>]) -> (f64, DVec3, DVec3) {
        let c = &self.list[i];
        let turned = |side: Side, start: DQuat| match side {
            Side::Node { member, node, .. } => states[member].orientation[node] * start.conjugate(),
            Side::Fixed(_) => DQuat::IDENTITY,
        };
        let mut r = turned(c.a, c.spin_start[0]) * turned(c.b, c.spin_start[1]).conjugate();
        if r.w < 0.0 {
            r = -r;
        }
        // The swing-twist split: the twist about n is 2·atan2(n·v, w), r = (w, v).
        let (n, v, w) = (c.normal, DVec3::new(r.x, r.y, r.z), r.w);
        let along = n.dot(v);
        let d = w * w + along * along;
        if d == 0.0 {
            return (0.0, n, -n);
        }
        let by_a = (n * (w * w) + v.cross(n) * w + v * along) / d;
        let by_b = (-n * (w * w) + v.cross(n) * w - v * along) / d;
        (2.0 * atan2(along, w), by_a, by_b)
    }

    /// The drilling friction's energy at `states` (J).
    pub(super) fn twist_energy(&self, states: &[MemberState<'_>]) -> f64 {
        (0..self.list.len())
            .map(|i| (i, self.twist_lever(i)))
            .filter(|&(_, lever)| lever > 0.0)
            .map(|(i, lever)| {
                let c = &self.list[i];
                let angle = self.twist(i, states).0.abs();
                c.friction * c.lambda * lever * smooth(angle, self.twist_smoothing).0
            })
            .sum()
    }

    /// Adds the drilling friction's torques at `states` to the members' gradient.
    pub(super) fn add_twist_gradient(
        &self,
        states: &[MemberState<'_>],
        gradient: &mut [Vec<DVec3>],
    ) {
        for i in 0..self.list.len() {
            let lever = self.twist_lever(i);
            if lever == 0.0 {
                continue;
            }
            let c = &self.list[i];
            let (angle, by_a, by_b) = self.twist(i, states);
            // dE/dφ = μ·λ·r·f₀′(|φ|)·sign(φ) = μ·λ·r·f₁·φ.
            let slope =
                c.friction * c.lambda * lever * smooth(angle.abs(), self.twist_smoothing).1 * angle;
            for (side, by) in [(c.a, by_a), (c.b, by_b)] {
                if let Side::Node { member, node, .. } = side {
                    let half = gradient[member].len() / 2;
                    gradient[member][half + node] += by * slope;
                }
            }
        }
    }

    /// Sets each contact's drilling stiffness for the matrices at `states`: μ·λ·r·f₀″ at its
    /// twist then (sticking: 2/e; sliding: none, its torque constant).
    pub(super) fn set_twist_stiffness(&mut self, states: &[MemberState<'_>]) {
        for i in 0..self.list.len() {
            let lever = self.twist_lever(i);
            let stiffness = if lever > 0.0 {
                let angle = self.twist(i, states).0.abs();
                let c = &self.list[i];
                c.friction * c.lambda * lever * smooth(angle, self.twist_smoothing).2
            } else {
                0.0
            };
            self.list[i].twist_stiffness = stiffness;
        }
    }

    /// Sets the orientations the contacts' twists are measured from (the start of a step of
    /// `h` seconds), and the twist within which they stick.
    pub(super) fn start_twist(&mut self, start: &[MemberState<'_>], h: f64) {
        self.twist_smoothing = SPIN_SLIP * h;
        let rigid = |side: Side| match side {
            Side::Node { member, .. } => start[member].position.len() == 1,
            Side::Fixed(_) => true,
        };
        for c in &mut self.list {
            c.drills = c.rolls() && rigid(c.a) && rigid(c.b);
            let at = |side: Side| match side {
                Side::Node { member, node, .. } => start[member].orientation[node],
                Side::Fixed(_) => DQuat::IDENTITY,
            };
            c.spin_start = [at(c.a), at(c.b)];
        }
    }
}
