//! Struts in a group's minimisation (`world/strut.rs`), beside its joints: each strut's length
//! with its rows, its energy, gradient and Gauss–Newton blocks.
//!
//! In the step the strut is a spring of K = k + c/β towards r* = (k·rest + (c/β)·d̂)/K (its
//! damper acts on the same length, and the length's rate is (d − d̂)/β), clamped to push only
//! and at most its most: a one-sided Huber spring, convex. Its stored energy is the spring's
//! alone (k towards its rest, the same clamp): the damper stores none.
//!
//! Each side's point moves within the step with its side's turn to first order from where it
//! was as the step began: p = x + a₀ + θ × a₀, a₀ its arm then and θ = log(q·q₀⁻¹) its turn
//! since (the rows by the solver's turn δ, θ moving by J⁻¹(θ)·δ). A sole presses a body where
//! the body lies under it: a round body rolling under a foot is pressed through its axis however
//! it turns, and a plank pressed at its end is turned by the lever it was pressed at. (At a
//! point of the body turning with it, a person's 2 kN pressed a 2.5 kg spruce pole as an
//! inverted pendulum, −N·r = −50 N·m/rad about its axis, and the pole rolled out from under
//! the foot at 30–40 rad/s, G2.10.)

use super::group::Joints;
use super::push::{huber, pushing};
use super::rows::{Row, Scalar};
use crate::so3;
use crate::structure::group::{MemberState, StepInfo};
use crate::world::strut::Strut;
use creeer_math::glam::{DQuat, DVec3};

/// A strut of a group: its sides numbered as the group's members, whether each is a rigid
/// body (its blocks then go in its 6×6 matrix, else in its structure's), its length at x̂
/// (and its walk's along its axes there, and at the step's start) and its sides' arms and turns
/// as the step began (once the step is set up).
#[derive(Clone, Debug)]
pub(crate) struct GroupStrut {
    pub strut: Strut,
    pub rigid: [bool; 2],
    pub hat: Option<(f64, [f64; 2])>,
    pub from: [f64; 2],
    pub start: Option<[(DVec3, DQuat); 2]>,
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

/// Its sides' arms and turns at `states` (the step's start).
fn arms(s: &Strut, states: &[MemberState<'_>]) -> [(DVec3, DQuat); 2] {
    let side = |side: Option<(usize, usize)>, at: DVec3| {
        side.map_or((DVec3::ZERO, DQuat::IDENTITY), |(m, n)| {
            let q = states[m].orientation[n];
            (q * at, q)
        })
    };
    [side(Some(s.a), s.at_a), side(s.b, s.at_b)]
}

/// Strut `s`'s length along `n` at `states`, with its rows (by each side's translation and
/// turn): its points as they turned since `start` to first order (`arms`; as they are, before
/// the step is set up, where the two agree).
fn measured(
    s: &Strut,
    n: DVec3,
    states: &[MemberState<'_>],
    start: Option<&[(DVec3, DQuat); 2]>,
) -> Scalar {
    // A side's arm and its row's turn part (d by the solver's turn there).
    let arm = |k: usize, (m, node): (usize, usize), at: DVec3| -> (DVec3, DVec3) {
        let q = states[m].orientation[node];
        match start {
            Some(start) => {
                let (a0, q0) = start[k];
                let turn = so3::log(q * q0.conjugate());
                let slope = so3::inverse_left_jacobian(turn).transpose() * a0.cross(n);
                (a0 + turn.cross(a0), slope)
            }
            None => {
                let a = q * at;
                (a, a.cross(n))
            }
        }
    };
    let (arm_a, turn_a) = arm(0, s.a, s.at_a);
    let (pb, turn_b) = match s.b {
        Some(side) => {
            let (a, turn) = arm(1, side, s.at_b);
            (states[side.0].position[side.1] + a, turn)
        }
        None => (s.at_b, DVec3::ZERO),
    };
    Scalar {
        value: (states[s.a.0].position[s.a.1] + arm_a - pb).dot(n),
        a: row(n, turn_a),
        b: row(-n, -turn_b),
    }
}

/// The two-sided Huber spring: as [`huber`] either way, at most `most[1]` pushing and
/// `−most[0]` pulling.
fn huber_both(u: f64, k: f64, most: [f64; 2]) -> (f64, f64, f64) {
    if u >= 0.0 {
        huber(u, k, most[1])
    } else {
        let (energy, force, weight) = huber(-u, k, -most[0]);
        (energy, -force, weight)
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
            from: [0.0; 2],
            start: None,
        });
    }

    /// Whether a strut's walk asks for a speed (it does work: `walk_work`).
    pub(crate) fn walking(&self) -> bool {
        (self.struts.iter()).any(|g| {
            g.strut
                .walk
                .is_some_and(|w| w.speed.iter().any(|v| v.abs() > 0.0))
        })
    }

    /// The work the walks did over the substep that ended at `states` (J): the force each gives
    /// at no speed over the body (c·v asked, within its least and most) over the way A went
    /// over B's point. What more they give is their damper's drag, which only takes energy (as
    /// a grip's, `grip_work`). (Unbooked, the energy guard took a walk's gain for the step's own
    /// and stopped it: a person asked to walk on a crate went at 0.4 mm/s, G2.10.)
    pub(super) fn walk_work(&self, states: &[MemberState<'_>]) -> f64 {
        let mut work = 0.0;
        for g in &self.struts {
            let (Some(walk), Some(start)) = (g.strut.walk, g.start.as_ref()) else {
                continue;
            };
            for (j, axis) in walk.axes.into_iter().enumerate() {
                let [least, most] = walk.most[j];
                let drive = (walk.damping * walk.speed[j]).clamp(least, most);
                let went = measured(&g.strut, axis, states, Some(start)).value - g.from[j];
                work += drive * went;
            }
        }
        work
    }

    /// Whether the group has struts.
    pub(crate) fn has_struts(&self) -> bool {
        !self.struts.is_empty()
    }

    /// Sets the struts' step up: their sides' arms at its start, their lengths at x̂ (and their
    /// walks' along its axes).
    pub(super) fn begin_struts(&mut self, info: &StepInfo<'_>) {
        for g in &mut self.struts {
            let (s, start) = (&g.strut, arms(&g.strut, info.start));
            let along = |n: DVec3| measured(s, n, info.hat, Some(&start)).value;
            let walk = s.walk.map_or([0.0; 2], |w| w.axes.map(along));
            g.hat = Some((along(s.direction), walk));
            g.from = s.walk.map_or([0.0; 2], |w| {
                w.axes
                    .map(|n| measured(s, n, info.start, Some(&start)).value)
            });
            g.start = Some(start);
        }
    }

    /// Each strut's terms at `states`: (strut, its length with its rows, the energy's
    /// derivative by the length, its stiffness; its walk's along its two axes after), and the
    /// energy stored and in all.
    pub(super) fn strut_terms(
        &self,
        states: &[MemberState<'_>],
        mut each: impl FnMut(usize, &Scalar, f64, f64),
    ) -> (f64, f64) {
        let (mut stored, mut total) = (0.0, 0.0);
        for (k, g) in self.struts.iter().enumerate() {
            let s = &g.strut;
            let d = measured(s, s.direction, states, g.start.as_ref());
            let spring = huber(s.rest - d.value, s.stiffness, s.most).0;
            stored += spring;
            // Before the step is set up (the energy guard measures first) only the spring is
            // known.
            let Some((hat, walk_hat)) = g.hat else {
                total += spring;
                continue;
            };
            let c = s.damping / self.beta();
            let stiff = s.stiffness + c;
            let rest = (s.stiffness * s.rest + c * hat) / stiff;
            // Its push falls with the speed it lengthens at (`push.rs`), if it has one.
            let (energy, force, weight) = if s.fastest.is_finite() {
                pushing(
                    d.value,
                    (stiff, rest),
                    s.most,
                    (hat, self.beta() * s.fastest),
                )
            } else {
                huber(rest - d.value, stiff, s.most)
            };
            total += energy;
            each(k, &d, -force, weight);
            // The walk: a damper of c towards the speed asked for, (ŝ + β·v − s)·c/β; it stores
            // nothing.
            let Some(walk) = s.walk else { continue };
            for (j, axis) in walk.axes.into_iter().enumerate() {
                let t = measured(s, axis, states, g.start.as_ref());
                let c = walk.damping / self.beta();
                let target = walk_hat[j] + self.beta() * walk.speed[j];
                let (energy, force, weight) = huber_both(target - t.value, c, walk.most[j]);
                total += energy;
                each(k, &t, -force, weight);
            }
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

    #[test]
    fn a_body_turned_under_a_strut_is_pressed_where_it_lies() {
        // B a pole of radius 25 mm whose top A presses down on, along its arm: turned about its
        // axis by up to 0.6 rad, the length is as it began (a sole on a round body), where a
        // point turning with it falls away by r·(1 − cos θ) (an inverted pendulum, G2.10).
        use super::{arms, measured};
        use crate::structure::group::MemberState;
        use crate::world::strut::Strut;
        use creeer_math::glam::{DQuat, DVec3};
        let strut = Strut {
            a: (0, 0),
            at_a: DVec3::ZERO,
            b: Some((1, 0)),
            at_b: DVec3::new(0.0, 0.025, 0.0),
            direction: DVec3::Y,
            rest: 1.0,
            stiffness: 30_000.0,
            damping: 0.0,
            most: 3600.0,
            fastest: f64::INFINITY,
            walk: None,
        };
        fn states<'s>(x: &'s [[DVec3; 1]; 2], q: &'s [[DQuat; 1]; 2]) -> [MemberState<'s>; 2] {
            [0, 1].map(|m| MemberState {
                position: &x[m],
                orientation: &q[m],
            })
        }
        let x = [[DVec3::new(0.0, 1.0, 0.0)], [DVec3::ZERO]];
        let start = arms(&strut, &states(&x, &[[DQuat::IDENTITY]; 2]));
        for theta in [0.1, 0.3, 0.6] {
            let q = [[DQuat::IDENTITY], [DQuat::from_rotation_x(theta)]];
            let s = states(&x, &q);
            let pressed = measured(&strut, DVec3::Y, &s, Some(&start)).value;
            let turning = measured(&strut, DVec3::Y, &s, None).value;
            assert!((pressed - 0.975).abs() < 1e-12, "{theta}: {pressed}");
            let fallen = 0.025 * (1.0 - creeer_math::det::cos(theta));
            assert!(
                (turning - (0.975 + fallen)).abs() < 1e-12,
                "{theta}: {turning}"
            );
        }
        // Its rows are its slope by the solver's turn δ (q → exp(δ)·q), turned 0.4 rad about a
        // slanting axis and pressed at a slanting arm: finite differences.
        let strut = Strut {
            at_b: DVec3::new(0.3, 0.025, -0.01),
            ..strut
        };
        let start = arms(&strut, &states(&x, &[[DQuat::IDENTITY]; 2]));
        let turned = crate::so3::exp(DVec3::new(0.2, -0.3, 0.1));
        let at = |delta: DVec3| {
            let q = [[DQuat::IDENTITY], [crate::so3::exp(delta) * turned]];
            measured(&strut, DVec3::Y, &states(&x, &q), Some(&start))
        };
        let row = at(DVec3::ZERO).b;
        for (k, axis) in [DVec3::X, DVec3::Y, DVec3::Z].into_iter().enumerate() {
            let e = 1e-6;
            let slope = (at(axis * e).value - at(axis * -e).value) / (2.0 * e);
            assert!(
                (slope - row[3 + k]).abs() < 1e-8,
                "{k}: {slope} against {}",
                row[3 + k]
            );
        }
    }

    #[test]
    fn the_walks_spring_pulls_and_pushes_alike() {
        use super::huber_both;
        assert_eq!(
            huber_both(-0.02, 1000.0, [-50.0, 50.0]),
            (0.2, -20.0, 1000.0)
        );
        assert_eq!(huber_both(0.02, 1000.0, [-50.0, 50.0]), (0.2, 20.0, 1000.0));
        // Each way its own most.
        assert_eq!(huber_both(-0.1, 1000.0, [-30.0, 50.0]).1, -30.0);
        assert_eq!(huber_both(0.1, 1000.0, [-30.0, 50.0]).1, 50.0);
    }
}
