//! Stepping several structures together, coupled by extra energy terms (contacts).
//!
//! The group's incremental potential is the sum of every member's own G plus the coupling
//! energy; it is minimised jointly (L-BFGS over all members, line search on the total).
//! Each member keeps its own preconditioner: its factorised matrix (co-rotated per node),
//! or for a rigid body (one node, no members) a dense 6×6 matrix that includes the
//! coupling's stiffness on that body.

use super::Structure;
use super::minimise::minimise;
use super::solver::{ImplicitSolver, Scheme, Timings};
use creeer_math::glam::{DMat3, DQuat, DVec3};
use std::time::Instant;

/// Positions and orientations of one member, as a coupling sees them.
#[derive(Clone, Copy, Debug)]
pub struct MemberState<'a> {
    /// Node positions.
    pub position: &'a [DVec3],
    /// Node orientations.
    pub orientation: &'a [DQuat],
}

/// What a coupling learns at the start of a step.
#[derive(Clone, Copy, Debug)]
pub struct StepInfo<'a> {
    /// β of the integration scheme (velocity = (x − x̂)/β).
    pub beta: f64,
    /// Time step (s).
    pub h: f64,
    /// Members at the start of the step.
    pub start: &'a [MemberState<'a>],
    /// Members at x̂, q̂ (the velocity reference: damping acts on the change from here).
    pub hat: &'a [MemberState<'a>],
    /// Members one step before the start (the history the schemes extrapolate from).
    pub previous: &'a [MemberState<'a>],
    /// How far ahead of the start x̂ lies (s): 0 for backward Euler, h/3 for BDF2, h/2 for
    /// Newmark.
    pub tau: f64,
}

/// Stiffness coupling two nodes of different members: (member, node) of each side and the
/// 6×6 block over (translation, rotation), rows of the first.
pub type NodeCoupling = ((usize, usize), (usize, usize), [[f64; 6]; 6]);

/// Energy terms coupling the members of a group.
pub trait Coupling: Sync {
    /// Called before every minimisation.
    fn begin(&mut self, info: &StepInfo<'_>);
    /// The largest remaining error of positions (m) the step may stop at, as far as the
    /// coupling's forces need it (the solver's own tolerance applies too).
    fn tolerance(&self) -> f64 {
        f64::INFINITY
    }
    /// Contribution to the incremental potential (elastic, damping, friction).
    fn energy(&self, states: &[MemberState<'_>]) -> f64;
    /// Stored elastic energy only (for the energy guard).
    fn elastic_energy(&self, states: &[MemberState<'_>]) -> f64;
    /// Adds the gradient of [`Self::energy`] to `gradient[member][block]` (translation
    /// blocks, then rotation blocks).
    fn add_gradient(&self, states: &[MemberState<'_>], gradient: &mut [Vec<DVec3>]);
    /// Gauss-Newton stiffness the coupling adds to node 0 of a rigid-body member, as a 6×6
    /// matrix over (translation, rotation), already including the factor of its damping.
    fn rigid_stiffness(&self, member: usize) -> [[f64; 6]; 6];
    /// Gauss-Newton stiffness coupling two nodes of different members, per contact: the
    /// 6×6 block over (translation, rotation) of `(member a, node a)` (rows) and `(member b,
    /// node b)` (columns), with the same factors as [`Self::rigid_stiffness`].
    fn rigid_coupling(&self) -> Vec<NodeCoupling> {
        Vec::new()
    }
    /// Translation stiffness the coupling adds at nodes of a structure member (3×3 blocks,
    /// possibly several per node), for its factorised matrix.
    fn node_stiffness(&self, _member: usize) -> Vec<(usize, DMat3)> {
        Vec::new()
    }
    /// Updates lagged quantities (friction's normal force) to `states`; returns whether any
    /// changed enough that the minimisation should continue with the new values.
    fn update_lag(&mut self, _states: &[MemberState<'_>]) -> bool {
        false
    }
}

/// No coupling: a single structure on its own.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoCoupling;

impl Coupling for NoCoupling {
    fn begin(&mut self, _: &StepInfo<'_>) {}
    fn energy(&self, _: &[MemberState<'_>]) -> f64 {
        0.0
    }
    fn elastic_energy(&self, _: &[MemberState<'_>]) -> f64 {
        0.0
    }
    fn add_gradient(&self, _: &[MemberState<'_>], _: &mut [Vec<DVec3>]) {}
    fn rigid_stiffness(&self, _: usize) -> [[f64; 6]; 6] {
        [[0.0; 6]; 6]
    }
}

/// One structure and its solver.
pub type Member<'a> = (&'a mut Structure, &'a mut ImplicitSolver);

pub(super) fn states<'a>(members: &'a [Member<'_>]) -> Vec<MemberState<'a>> {
    members
        .iter()
        .map(|(s, _)| MemberState {
            position: &s.position,
            orientation: &s.orientation,
        })
        .collect()
}

pub(super) fn trial_states<'a>(members: &'a [Member<'_>]) -> Vec<MemberState<'a>> {
    members
        .iter()
        .map(|(_, v)| MemberState {
            position: &v.work.trial_x,
            orientation: &v.work.trial_q,
        })
        .collect()
}

/// Energy may rise by this fraction of the kinetic + elastic energy (before or after the
/// step, whichever is larger) before the guard rejects a Newmark step (rounding, Newmark's
/// bounded energy oscillation, and its second-order error in fields that vary in space).
const ENERGY_SLACK: f64 = 1.0e-3;

/// Total energy (kinetic, elastic, potential of gravity and loads, coupling) and its
/// kinetic + elastic part. Member elastic energies come from the last minimisation when it
/// has `just_minimised` the members, or when the state is unchanged since (checked bit for
/// bit). Right after a minimisation only its value is right: the step's plastic flow is not
/// yet permanent, and the members' own elastic energy would count it as stored.
fn group_energy(
    members: &[Member<'_>],
    coupling: &dyn Coupling,
    just_minimised: bool,
) -> (f64, f64) {
    let mut total = (0.0, 0.0);
    for (s, v) in members {
        let unchanged =
            just_minimised || (s.position == v.snapshot.end_x && s.orientation == v.snapshot.end_q);
        let elastic = if unchanged {
            v.work.current_elastic
        } else {
            s.elastic_energy()
        };
        let (kinetic, potential) = s.node_energy();
        total.0 += kinetic + elastic + potential;
        total.1 += kinetic + elastic;
    }
    let contact = coupling.elastic_energy(&states(members));
    (total.0 + contact, total.1 + contact)
}

/// Where a member started a substep, and its dashpots' forces then.
type Start = (Vec<DVec3>, Vec<DQuat>, Vec<(usize, DVec3, DVec3)>);

/// The work the members' external loads and torques did since they were at `start` (J) that
/// the energy's potential of the loads does not count. A torque has no potential as a load
/// has (rotations do not add): its work, τ·Δθ for the rotation vector Δθ a node turned by,
/// is credited instead; without that every joule a torque gave was taken for the
/// integration's gain and removed (a motor in a free-floating station turned it for a
/// second, then no more: F6). Newmark's forces are the mean of the step's two ends: a load
/// or torque that changed did the work of its mean with the last step's, which the potential
/// (at the load now) misses by ½·(f₋₁ − f)·Δx: unbooked, a thruster or motor switched on or
/// off took the step for a gain and redid it by backward Euler.
fn unbooked_work(members: &[Member<'_>], start: &[Start], newmark: bool) -> f64 {
    let mut work = 0.0;
    for ((s, v), (x0, q0, dashpots)) in members.iter().zip(start) {
        for i in 0..s.node_count() {
            let (torque, before) = (s.torque[i], v.previous_torque[i]);
            let torque = if newmark {
                0.5 * (torque + before)
            } else {
                torque
            };
            if torque != DVec3::ZERO {
                work += torque.dot(crate::so3::log(s.orientation[i] * q0[i].conjugate()));
            }
            let change = v.previous_load[i] - s.load[i];
            if newmark && change != DVec3::ZERO {
                work += 0.5 * change.dot(s.position[i] - x0[i]);
            }
        }
        work += s.dashpot_work(x0, q0);
        // Newmark's mean took the last step's dashpots' forces too (in its accelerations).
        if newmark {
            for &(n, force, torque) in dashpots {
                work += 0.5
                    * (force.dot(s.position[n] - x0[n])
                        + torque.dot(crate::so3::log(s.orientation[n] * q0[n].conjugate())));
            }
        }
    }
    work
}

/// Advances every member by one step of its solver, solving them jointly with `coupling`.
/// All members must use the same scheme, time step and substeps.
///
/// # Panics
/// If the members' solvers differ in scheme or time step.
pub fn step_group(members: &mut [Member<'_>], coupling: &mut dyn Coupling) {
    step_group_with(members, coupling, None);
}

/// Like [`step_group`], with every substep using `scheme` instead of the solvers' own one
/// (backward Euler for impacts: see `world::impact`).
///
/// # Panics
/// If the members' solvers differ in scheme or time step.
pub fn step_group_with(
    members: &mut [Member<'_>],
    coupling: &mut dyn Coupling,
    scheme: Option<Scheme>,
) {
    let start = Instant::now();
    let Some((_, first)) = members.first() else {
        return;
    };
    let (own, h, substeps, guard) = (first.scheme, first.h, first.substeps, first.energy_guard);
    let scheme = scheme.unwrap_or(own);
    for (_, v) in members.iter_mut() {
        #[allow(clippy::float_cmp)] // the very same time step, not a close one
        let same = v.scheme == own && v.h == h && v.substeps == substeps;
        assert!(same, "one time step per group");
        v.last_iterations = 0;
        v.timings = Timings::default();
    }
    for _ in 0..substeps {
        if guard {
            let t = Instant::now();
            let (before, active) = group_energy(members, coupling, false);
            for (s, v) in members.iter_mut() {
                v.save(s);
            }
            // Whether any load or torque does work the loads' potential misses.
            let acting = members.iter().any(|(s, v)| {
                s.torque
                    .iter()
                    .chain(&v.previous_torque)
                    .any(|t| *t != DVec3::ZERO)
                    || s.load != v.previous_load
                    || !s.dashpots.is_empty()
            });
            let start: Vec<Start> = if acting {
                members
                    .iter()
                    .map(|(s, v)| {
                        (
                            s.position.clone(),
                            s.orientation.clone(),
                            v.dashpot_end.clone(),
                        )
                    })
                    .collect()
            } else {
                Vec::new()
            };
            // The energy after a substep by `scheme`, less the work the loads' potential misses.
            let net = |members: &[Member<'_>], energy: f64, scheme: Scheme| {
                if acting {
                    energy - unbooked_work(members, &start, scheme == Scheme::Newmark)
                } else {
                    energy
                }
            };
            members[0].1.timings.guard += t.elapsed();
            substep(members, coupling, scheme);
            let t = Instant::now();
            let (after, active_after) = group_energy(members, coupling, true);
            let after = net(members, after, scheme);
            members[0].1.timings.guard += t.elapsed();
            // The slack scales with the energy in motion before or after the step, whichever
            // is larger: a body starting from rest in a varying field (planet gravity) makes
            // a genuine second-order energy error that the 1 nJ floor alone would reject.
            let limit = before + ENERGY_SLACK * active.max(active_after) + 1e-9;
            if after > limit {
                // Fall back to ever more dissipative schemes; in non-convex contact situations
                // even BDF2 can gain a little. As a last resort the excess is taken from the
                // kinetic energy.
                let mut after = after;
                let fallbacks: &[Scheme] = match scheme {
                    Scheme::Newmark => &[Scheme::Bdf2, Scheme::BackwardEuler],
                    Scheme::Bdf2 => &[Scheme::BackwardEuler],
                    Scheme::BackwardEuler => &[],
                };
                for &fallback in fallbacks {
                    for (s, v) in members.iter_mut() {
                        v.restore(s);
                        v.fallbacks += 1;
                    }
                    substep(members, coupling, fallback);
                    after = net(members, group_energy(members, coupling, true).0, fallback);
                    if after <= limit {
                        break;
                    }
                }
                if after > limit {
                    remove_kinetic(members, after - before);
                }
            }
            for (s, v) in members.iter_mut() {
                v.snapshot.end_x.clone_from(&s.position);
                v.snapshot.end_q.clone_from(&s.orientation);
            }
        } else {
            substep(members, coupling, scheme);
        }
        // What the next substep's prediction carries: these loads.
        for (s, v) in members.iter_mut() {
            v.previous_load.clone_from(&s.load);
            v.previous_torque.clone_from(&s.torque);
        }
    }
    // Plastic flow and fracture, on the converged state.
    for (s, v) in members.iter_mut() {
        let (yielding, wanted) = s.yield_members(&v.work);
        v.wanted.extend(wanted);
        if yielding.yielded > 0 {
            // The elastic energy remembered for the energy guard is no longer valid.
            v.snapshot.end_x.clear();
        }
        if yielding.broke > 0 {
            v.refactor(s);
        }
        v.last_yielding = yielding;
    }
    members[0].1.timings.total = start.elapsed();
}

fn substep(members: &mut [Member<'_>], coupling: &mut dyn Coupling, scheme: Scheme) {
    let t = Instant::now();
    let start: Vec<(Vec<DVec3>, Vec<DQuat>)> = members
        .iter()
        .map(|(s, _)| (s.position.clone(), s.orientation.clone()))
        .collect();
    // The history before the prediction moves it on.
    let previous: Vec<(Vec<DVec3>, Vec<DQuat>)> = members
        .iter()
        .map(|(_, v)| (v.previous_x.clone(), v.previous_q.clone()))
        .collect();
    for (s, v) in members.iter_mut() {
        v.begin_substep(s, scheme);
    }
    let start_states: Vec<MemberState<'_>> = start
        .iter()
        .map(|(x, q)| MemberState {
            position: x,
            orientation: q,
        })
        .collect();
    let hat: Vec<MemberState<'_>> = members
        .iter()
        .map(|(_, v)| MemberState {
            position: &v.work.x_hat,
            orientation: &v.work.q_hat,
        })
        .collect();
    let previous_states: Vec<MemberState<'_>> = previous
        .iter()
        .map(|(x, q)| MemberState {
            position: x,
            orientation: q,
        })
        .collect();
    let (beta, h) = (members[0].1.work.beta, members[0].1.h);
    coupling.begin(&StepInfo {
        beta,
        h,
        start: &start_states,
        hat: &hat,
        previous: &previous_states,
        tau: members[0].1.work.tau,
    });
    members[0].1.timings.predict += t.elapsed();
    minimise(members, coupling);
    for (s, v) in members.iter_mut() {
        v.finish_substep(s);
    }
}

/// Scales all velocities of the group so that its kinetic energy drops by `excess`.
fn remove_kinetic(members: &mut [Member<'_>], excess: f64) {
    let kinetic: f64 = members.iter().map(|(s, _)| s.node_energy().0).sum();
    if kinetic <= 0.0 {
        return;
    }
    let factor = ((kinetic - excess).max(0.0) / kinetic).sqrt();
    for (s, _) in members.iter_mut() {
        for v in s.velocity.iter_mut().chain(s.angular_velocity.iter_mut()) {
            *v *= factor;
        }
    }
}
