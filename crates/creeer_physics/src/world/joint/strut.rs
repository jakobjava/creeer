//! Struts in a group's minimisation (`world/strut.rs`), beside its joints: each strut's length
//! with its rows, its energy, gradient and Gauss–Newton blocks.
//!
//! In the step the strut is a spring of K = k + c/β towards r* = (k·rest + (c/β)·d̂)/K (its
//! damper acts on the same length, and the length's rate is (d − d̂)/β), clamped to push only
//! and at most its most: a one-sided Huber spring, convex. Its stored energy is the spring's
//! alone (k towards its rest, the same clamp): the damper stores none.

use super::group::Joints;
use super::rows::{Row, Scalar};
use crate::structure::group::{MemberState, StepInfo};
use crate::world::strut::Strut;
use creeer_math::glam::{DQuat, DVec3};

/// A strut of a group: its sides numbered as the group's members, whether each is a rigid
/// body (its blocks then go in its 6×6 matrix, else in its structure's), and its length at x̂
/// (once the step is set up).
#[derive(Clone, Debug)]
pub(crate) struct GroupStrut {
    pub strut: Strut,
    pub rigid: [bool; 2],
    pub hat: Option<f64>,
}

fn row(translation: DVec3, turn: DVec3) -> Row {
    [
        translation.x,
        translation.y,
        translation.z,
        turn.x,
        turn.y,
        turn.z,
    ]
}

/// Strut `s`'s length at `states`, with its rows (by each side's translation and turn).
fn measured(s: &Strut, states: &[MemberState<'_>]) -> Scalar {
    let pose = |(m, n): (usize, usize)| -> (DVec3, DQuat) {
        (states[m].position[n], states[m].orientation[n])
    };
    let (xa, qa) = pose(s.a);
    let arm_a = qa * s.at_a;
    let (pb, arm_b) = match s.b {
        Some(side) => {
            let (xb, qb) = pose(side);
            let arm = qb * s.at_b;
            (xb + arm, arm)
        }
        None => (s.at_b, DVec3::ZERO),
    };
    let n = s.direction;
    Scalar {
        value: (xa + arm_a - pb).dot(n),
        a: row(n, arm_a.cross(n)),
        b: row(-n, -arm_b.cross(n)),
    }
}

/// A one-sided Huber spring's energy at compression `u` with stiffness `k`, at most `most`;
/// its force and its stiffness there.
fn huber(u: f64, k: f64, most: f64) -> (f64, f64, f64) {
    if u <= 0.0 || k <= 0.0 {
        (0.0, 0.0, 0.0)
    } else if k * u <= most {
        (0.5 * k * u * u, k * u, k)
    } else {
        (most * u - 0.5 * most * most / k, most, 0.0)
    }
}

impl Joints {
    /// Adds a strut (its sides numbered as the group's members; `rigid`: whether each side
    /// is a rigid body).
    pub(crate) fn add_strut(&mut self, strut: Strut, rigid: [bool; 2]) {
        self.struts.push(GroupStrut {
            strut,
            rigid,
            hat: None,
        });
    }

    /// Whether the group has struts.
    pub(crate) fn has_struts(&self) -> bool {
        !self.struts.is_empty()
    }

    /// Sets the struts' step up: their lengths at x̂.
    pub(super) fn begin_struts(&mut self, info: &StepInfo<'_>) {
        for g in &mut self.struts {
            g.hat = Some(measured(&g.strut, info.hat).value);
        }
    }

    /// Each strut's terms at `states`: (strut, its length with its rows, the energy's
    /// derivative by the length, its stiffness), and the energy stored and in all.
    pub(super) fn strut_terms(
        &self,
        states: &[MemberState<'_>],
        mut each: impl FnMut(usize, &Scalar, f64, f64),
    ) -> (f64, f64) {
        let (mut stored, mut total) = (0.0, 0.0);
        for (k, g) in self.struts.iter().enumerate() {
            let s = &g.strut;
            let d = measured(s, states);
            let spring = huber(s.rest - d.value, s.stiffness, s.most).0;
            stored += spring;
            // Before the step is set up (the energy guard measures first) only the spring is
            // known.
            let Some(hat) = g.hat else {
                total += spring;
                continue;
            };
            let c = s.damping / self.beta();
            let stiff = s.stiffness + c;
            let rest = (s.stiffness * s.rest + c * hat) / stiff;
            let (energy, force, weight) = huber(rest - d.value, stiff, s.most);
            total += energy;
            each(k, &d, -force, weight);
        }
        (stored, total)
    }

    /// Strut `k`'s sides.
    pub(super) fn strut_sides(&self, k: usize) -> [(Option<(usize, usize)>, bool); 2] {
        let g = &self.struts[k];
        [(Some(g.strut.a), g.rigid[0]), (g.strut.b, g.rigid[1])]
    }
}

#[cfg(test)]
mod tests {
    use super::huber;

    #[test]
    fn the_spring_pushes_only_and_at_most_its_most() {
        assert_eq!(huber(-0.1, 1000.0, 50.0), (0.0, 0.0, 0.0));
        assert_eq!(huber(0.02, 1000.0, 50.0), (0.2, 20.0, 1000.0));
        // Beyond the cap: the force constant, the energy on, continuous at the cap.
        let (e, f, k) = huber(0.1, 1000.0, 50.0);
        assert_eq!((f, k), (50.0, 0.0));
        assert!((e - (50.0 * 0.1 - 0.5 * 2500.0 / 1000.0)).abs() < 1e-12);
        assert!((huber(0.05, 1000.0, 50.0).0 - 1.25).abs() < 1e-12);
    }
}
