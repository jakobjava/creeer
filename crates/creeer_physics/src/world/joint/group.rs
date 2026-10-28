//! The joints of one group as terms of its minimisation (`joint.rs`): their energy, its
//! gradient, the multipliers' refresh and their Gauss–Newton blocks, all from the joints'
//! scalars and rows (`rows.rs`). Each joint is measured where its terms are taken, one at a
//! time (measured all at once into a list, a ragdoll's terms cost three times as much).

use super::rows::{At, Row, Scalar};
use super::{ARM, HOLDS, Joint, SETTLED, STIFFER};
use crate::structure::group::{MemberState, NodeCoupling, StepInfo};
use creeer_math::glam::{DMat3, DVec3};

/// The friction's smoothing: the arms' gap within which it sticks (m; 10⁻⁴ rad of turn).
pub(super) const STICK: f64 = 1e-5;

/// The joints of a group.
#[derive(Clone, Debug, Default)]
pub(crate) struct Joints {
    /// Each joint: its index in the world, and the joint with its sides as members.
    pub(crate) list: Vec<(usize, Joint)>,
    /// Each joint's sides: mass and inertia (kg, kg·m²; infinite for the world), and whether
    /// it is a rigid body (its blocks then go in its 6×6 matrix, else in its structure's).
    pub(super) sides: Vec<[(f64, f64, bool); 2]>,
    /// Each joint's penalties for the step: points, turns (N/m, N·m/rad).
    pub(super) stiffness: Vec<(f64, f64)>,
    beta: f64,
    /// The substep (s).
    pub(super) h: f64,
    /// Each joint's arms' gaps at x̂ (damping's reference).
    hat: Vec<Option<[[f64; 3]; 3]>>,
    /// The blocks for the step's matrices: rigid bodies' own (by member), between nodes (one
    /// per joint between two bodies), and structures' nodes' translations.
    pub(crate) rigid: Vec<[[f64; 6]; 6]>,
    pub(crate) cross: Vec<NodeCoupling>,
    pub(super) nodes: Vec<(usize, usize, DMat3)>,
    /// The group's struts (`strut.rs`): their terms ride with the joints'.
    pub(crate) struts: Vec<super::strut::GroupStrut>,
    /// The group's grips (`grip.rs`): their terms ride with the joints'.
    pub(crate) grips: Vec<super::grip::GroupGrip>,
    /// The work its moved points did over the last substep (J; `project.rs`).
    pub(super) driven: f64,
}

/// f₀ and f₀'/x of the friction's smoothing (the contacts' cubic).
fn smooth(x: f64) -> (f64, f64) {
    let e = STICK;
    if x >= e {
        (x, 1.0 / x)
    } else {
        (
            -x * x * x / (3.0 * e * e) + x * x / e + e / 3.0,
            2.0 / e - x / (e * e),
        )
    }
}

/// The energy of a one-sided constraint `g ≤ 0` with multiplier `lambda` and penalty `k`
/// (augmented Lagrangian), and its derivative.
fn one_sided(g: f64, lambda: f64, k: f64) -> (f64, f64) {
    let f = (lambda + k * g).max(0.0);
    ((f * f - lambda * lambda) / (2.0 * k), f)
}

/// A drive's energy: a Huber spring on each arm (k·r up to the strength's share, constant
/// beyond), telling `each` every scalar's coefficient and weight.
fn drive_energy(
    drive: super::Drive,
    arms: &[[Scalar; 3]; 3],
    mut each: impl FnMut(&Scalar, (f64, f64)),
) -> f64 {
    let k = drive.stiffness / (2.0 * ARM * ARM);
    let most = drive.strength / (2.0 * ARM);
    let mut total = 0.0;
    for arm in arms {
        let r = arm.iter().map(|s| s.value * s.value).sum::<f64>().sqrt();
        let (energy, per) = if k * r <= most {
            (0.5 * k * r * r, k)
        } else {
            (most * r - 0.5 * most * most / k, most / r)
        };
        total += energy;
        for s in arm {
            each(s, (per * s.value, per));
        }
    }
    total
}

impl Joints {
    /// The group's joints, with each side's mass, inertia and whether it is a rigid body.
    pub(crate) fn new(list: Vec<(usize, Joint)>, sides: Vec<[(f64, f64, bool); 2]>) -> Self {
        Self {
            list,
            sides,
            ..Self::default()
        }
    }

    /// Whether any joint has a limit (its stops are stepped as contacts are: BDF2).
    pub(crate) fn limited(&self) -> bool {
        self.list.iter().any(|(_, j)| match j.kind {
            super::Kind::Ball { cone, twist } => cone.is_some() || twist.is_some(),
            super::Kind::Hinge { range } => range.is_some(),
        })
    }

    /// Sets the step up: the penalties, the arms' references, the blocks.
    pub(crate) fn begin(&mut self, info: &StepInfo<'_>) {
        (self.beta, self.h) = (info.beta, info.h);
        let reduced = |a: f64, b: f64| if b.is_finite() { a * b / (a + b) } else { a };
        let b2 = info.beta * info.beta;
        self.stiffness = (self.sides.iter())
            .map(|[(ma, ia, _), (mb, ib, _)]| {
                (
                    STIFFER * reduced(*ma, *mb) / b2,
                    STIFFER * reduced(*ia, *ib) / b2,
                )
            })
            .collect();
        self.hat = (self.list.iter())
            .map(|(_, j)| {
                (j.measured(info.hat, false).arms).map(|a| a.map(|arm| arm.map(|s| s.value)))
            })
            .collect();
        self.begin_struts(info);
        self.begin_grips(info);
        // Friction sticks where a joint first meets it.
        for (_, joint) in &mut self.list {
            if joint.friction > 0.0 && joint.anchor.is_none() {
                joint.anchor = Some(joint.relative(info.start));
            }
        }
        self.make_blocks(info.start);
    }

    /// Each joint's energy terms at `states` (with the scalars' rows if `rows`): (joint,
    /// scalar, coefficient, weight) for every scalar it uses, the coefficient the energy's
    /// derivative by it and the weight its Gauss–Newton stiffness; and the energy, stored
    /// (constraints hold none: they are ideal) and in all.
    pub(super) fn terms(
        &self,
        states: &[MemberState<'_>],
        rows: bool,
        mut each: impl FnMut(usize, &Scalar, f64, f64),
    ) -> (f64, f64) {
        let (mut stored, mut total) = (0.0, 0.0);
        for (j, (_, joint)) in self.list.iter().enumerate() {
            let at = joint.measured(states, rows);
            // Before the step is set up (the energy guard measures first) only the drives,
            // which alone store energy, are known.
            let Some(&(kt, kr)) = self.stiffness.get(j) else {
                if let (Some(drive), Some(arms)) = (joint.drive, &at.drive) {
                    let e = drive_energy(drive, arms, |_, _| {});
                    stored += e;
                    total += e;
                }
                continue;
            };
            total += self.constraint_terms(j, joint, &at, (kt, kr), &mut each);
            if let (Some(drive), Some(arms)) = (joint.drive, &at.drive) {
                let e = drive_energy(drive, arms, |s, (coefficient, weight)| {
                    each(j, s, coefficient, weight);
                });
                stored += e;
                total += e;
            }
        }
        (stored, total)
    }

    /// Joint `j`'s terms but its drive's: its points, axes and stops, its damping and its
    /// friction (their energy, telling `each` as `terms`).
    fn constraint_terms(
        &self,
        j: usize,
        joint: &Joint,
        at: &At,
        (kt, kr): (f64, f64),
        each: &mut impl FnMut(usize, &Scalar, f64, f64),
    ) -> f64 {
        let mut total = 0.0;
        let m = joint.multipliers;
        for (k, s) in at.point.iter().enumerate() {
            let (lambda, g) = (m.point[k], s.value);
            total += lambda * g + 0.5 * kt * g * g;
            each(j, s, lambda + kt * g, kt);
        }
        for (k, s) in at.axis.iter().flatten().enumerate() {
            let (lambda, g) = (m.axis[k], s.value);
            total += lambda * g + 0.5 * kr * g * g;
            each(j, s, lambda + kr * g, kr);
        }
        for (k, s) in at.limits.iter().enumerate() {
            let Some(s) = s else { continue };
            let (e, f) = one_sided(s.value, m.limits[k], kr);
            total += e;
            each(j, s, f, if f > 0.0 { kr } else { 0.0 });
        }
        // Damping of the arms' gaps from x̂: c·(d − d̂)/β, c from the turn's damping (three
        // arms turned by θ part by √2·ARM·θ in all).
        if let (Some(arms), Some(Some(hat))) = (&at.arms, self.hat.get(j)) {
            let c = joint.damping / (2.0 * ARM * ARM) / self.beta;
            for (arm, hat) in arms.iter().zip(hat) {
                for (s, h) in arm.iter().zip(hat) {
                    let d = s.value - h;
                    total += 0.5 * c * d * d;
                    each(j, s, c * d, c);
                }
            }
        }
        // Friction on the arms' gaps from where it sticks (its anchor): μ·|slip| smoothed, μ
        // from the friction torque (three arms turned by θ part by √2·ARM·θ in all).
        if let Some(arms) = &at.stick {
            let mu = joint.friction / (std::f64::consts::SQRT_2 * ARM);
            let slip = (arms.iter().flatten())
                .map(|s| s.value * s.value)
                .sum::<f64>()
                .sqrt();
            let (f0, f1) = smooth(slip);
            total += mu * f0;
            for s in arms.iter().flatten() {
                each(j, s, mu * f1 * s.value, mu * f1);
            }
        }
        total
    }

    /// Their contribution to the step's objective (the struts' with it).
    pub(crate) fn energy(&self, states: &[MemberState<'_>]) -> f64 {
        let timer = crate::profile::start();
        let energy = self.terms(states, false, |_, _, _, _| {}).1
            + self.strut_terms(states, |_, _, _, _| {}).1
            + self.grip_terms(states, |_, _, _, _| {});
        timer.stop(crate::profile::Slot::JointTerms);
        energy
    }

    /// The energy they store (their drives' and the struts' springs; constraints and limits
    /// are ideal).
    pub(crate) fn elastic_energy(&self, states: &[MemberState<'_>]) -> f64 {
        let (drives, struts) = self.stored(states);
        drives + struts
    }

    /// The energy their drives store, and the struts' and grips' (set anew each step: the
    /// guard counts it by its change).
    pub(crate) fn stored(&self, states: &[MemberState<'_>]) -> (f64, f64) {
        let drives = self.terms(states, false, |_, _, _, _| {}).0;
        let held = self.strut_terms(states, |_, _, _, _| {}).0 + self.grip_stored(states);
        (drives, held)
    }

    /// The step's β (its velocities are the change from x̂ over it).
    pub(super) fn beta(&self) -> f64 {
        self.beta
    }

    /// Adds their gradient (translation blocks, then rotation blocks, per member).
    pub(crate) fn add_gradient(&self, states: &[MemberState<'_>], gradient: &mut [Vec<DVec3>]) {
        let timer = crate::profile::start();
        let mut add = |(m, n): (usize, usize), row: &Row, coefficient: f64| {
            let half = gradient[m].len() / 2;
            gradient[m][n] += DVec3::new(row[0], row[1], row[2]) * coefficient;
            gradient[m][half + n] += DVec3::new(row[3], row[4], row[5]) * coefficient;
        };
        self.terms(states, true, |j, s, coefficient, _| {
            let joint = &self.list[j].1;
            add(joint.a, &s.a, coefficient);
            if let Some(b) = joint.b {
                add(b, &s.b, coefficient);
            }
        });
        self.strut_terms(states, |k, s, coefficient, _| {
            let [(a, _), (b, _)] = self.strut_sides(k);
            for (side, row) in [(a, &s.a), (b, &s.b)] {
                if let Some(side) = side {
                    add(side, row, coefficient);
                }
            }
        });
        self.grip_terms(states, |k, s, coefficient, _| {
            add(self.grip_side(k).0, &s.a, coefficient);
        });
        timer.stop(crate::profile::Slot::JointTerms);
    }

    /// Refreshes the multipliers at `states` (the augmented Lagrangian's update); returns
    /// whether a joint did not yet hold: its gap over [`HOLDS`] and the refresh changing its
    /// force by more than [`SETTLED`] of it (a joint carrying a body's weight between stiff
    /// contacts gains little by each refresh, and refreshing it for micrometres that move its
    /// force by less took a ragdoll lying on the ground every refresh its steps allow).
    pub(crate) fn update_lag(&mut self, states: &[MemberState<'_>]) -> bool {
        let mut moved = false;
        let open = |gap: f64, change: f64, force: f64| {
            gap.abs() > HOLDS && change.abs() > SETTLED * force.abs()
        };
        for (j, (_, joint)) in self.list.iter_mut().enumerate() {
            let at = joint.measured(states, false);
            let (kt, kr) = self.stiffness[j];
            let m = &mut joint.multipliers;
            let (point, axis) = (m.point.length(), m.axis.length());
            for (k, s) in at.point.iter().enumerate() {
                m.point[k] += kt * s.value;
                moved |= open(s.value, kt * s.value, point);
            }
            for (k, s) in at.axis.iter().flatten().enumerate() {
                m.axis[k] += kr * s.value;
                moved |= open(s.value, kr * s.value, axis);
            }
            for (k, s) in at.limits.iter().enumerate() {
                let new = s.map_or(0.0, |s| (m.limits[k] + kr * s.value).max(0.0));
                moved |= open((new - m.limits[k]) / kr, new - m.limits[k], m.limits[k]);
                m.limits[k] = new;
            }
        }
        if moved {
            self.make_blocks(states);
        }
        moved
    }
}
