//! Time stepping of a structure: implicit integration as an energy minimisation (ADR-0004).
//!
//! Each step finds the positions and orientations that minimise
//!
//! G = Σ m/(2β²)·|x − x*|² + Σ 1/(2β²)·Δφᵀ·J·Δφ + E_elastic + D_damping − W_external,
//!
//! where x* and q* are the predicted positions and orientations (Δφ = log(q·q*⁻¹)), with
//! Newton steps that must lower G (line search). The Newton matrix M + (β² + β·c)·K of the
//! rest shape is factorised once; forces are rotated into the body frame and back, so the
//! factorisation stays valid while the structure turns as a whole.
//!
//! Orientations are integrated on the rotation group: BDF2's x̂ = x + (x − x₋₁)/3 becomes
//! q̂ = exp(log(q·q₋₁⁻¹)/3)·q. Nodes are lumps of the members' mass and rotational inertia;
//! gyroscopic effects of these small nodal inertias are neglected (the rotation of the
//! structure as a whole is carried by the translating node masses, which is exact).

use super::Structure;
use super::element::MEASURES;
use super::step::Snapshot;
use crate::sparse::Cholesky;
use creeer_math::glam::{DMat3, DQuat, DVec3};
use std::time::Duration;

/// Implicit integration scheme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scheme {
    /// Backward Euler: first order, strong artificial damping. Used for statics.
    BackwardEuler,
    /// BDF2: second order, little artificial damping, but its period error grows quickly
    /// with frequency (−3.5 % at 2.7 Hz with 60 Hz steps).
    Bdf2,
    /// Newmark's average acceleration (trapezoidal rule): second order, no artificial
    /// damping and the smallest period error, about (ω·h)²/12 (−0.7 % at 2.7 Hz). It is a
    /// variational integrator (Kane et al. 2000), so energy stays bounded over long runs;
    /// high frequencies are damped by the materials' physical damping instead. The default.
    Newmark,
}

/// Where the time of one step went (for profiling and the perf overlay).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Timings {
    /// Energy checks and snapshots of the energy guard.
    pub guard: Duration,
    /// Prediction and Newton start.
    pub predict: Duration,
    /// Forces (gradient of G).
    pub residual: Duration,
    /// Evaluations of G (line search).
    pub objective: Duration,
    /// Sparse solves with L-BFGS.
    pub solve: Duration,
    /// Whole step.
    pub total: Duration,
}

/// When the Newton matrix is re-linearised at the current shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refactoring {
    /// Never: always the rest shape (in the body frame).
    Never,
    /// When a Newton step had to be cut below 1/4 by the line search: the shape has moved
    /// far from the one the matrix was built for (large local rotations). The default.
    WhenSlow,
    /// Before every Newton iteration (full Gauss-Newton; used for statics).
    EveryIteration,
}

/// Steps one [`Structure`] in time. Keeps the factorisation and the integration history.
#[derive(Clone, Debug)]
pub struct ImplicitSolver {
    pub(super) scheme: Scheme,
    pub(super) substeps: u32,
    pub(super) h: f64,
    /// Newton iterations per step at most: a safety net only (default 500). Steps always
    /// iterate until converged; see `tolerance`.
    pub max_iterations: u32,
    /// A step stops once the estimated remaining error is below this (m, or rad for
    /// rotations). Chosen so that the motion matches the fully converged solution
    /// (test `default_tolerance_matches_the_converged_solution`).
    pub tolerance: f64,
    /// Newton iterations used by the last step (summed over substeps).
    pub last_iterations: u32,
    /// Newton iterations of all its steps so far (not saved): the work a scene's ticks cost,
    /// the same on every machine (the sandbox's work gates).
    pub iterations_total: u64,
    /// Where the time of the last step went.
    pub timings: Timings,
    /// Plastic flow and fracture in the last step.
    pub last_yielding: super::Yielding,
    /// When to re-linearise (default [`Refactoring::WhenSlow`]).
    pub refactoring: Refactoring,
    /// Factorisations computed so far.
    pub factorisations: u32,
    /// Node orientations when the matrix was built.
    pub(super) factor_orientation: Vec<DQuat>,
    /// Whether the matrix was built at the current shape (else at the rest shape), the node
    /// positions then, and which members were broken then: with the orientations and the
    /// contact blocks, all the matrix was built from (a loaded world builds it again).
    pub(super) factor_current: bool,
    pub(super) factor_position: Vec<DVec3>,
    pub(super) factor_broken: Vec<bool>,
    /// Scratch: each node's rotation since then.
    pub(super) turns: Vec<DMat3>,
    /// Stiffness of the contacts at nodes (3×3 translation blocks, by node) in the
    /// factorised matrix.
    pub(super) contact_blocks: Vec<(usize, DMat3)>,
    pub(super) chol: Cholesky,
    /// The factor applied to the unit translations at nodes a coupled solve asked for (node,
    /// its three columns), solved once per factorisation (`coupled.rs`).
    pub(super) unit_columns: Vec<(usize, [Vec<DVec3>; 3])>,
    pub(super) previous_x: Vec<DVec3>,
    pub(super) previous_v: Vec<DVec3>,
    pub(super) previous_q: Vec<DQuat>,
    pub(super) previous_w: Vec<DVec3>,
    /// Accelerations at the end of the last step (Newmark).
    pub(super) acceleration: Vec<DVec3>,
    pub(super) angular_acceleration: Vec<DVec3>,
    /// The external loads and torques of the last step: Newmark carries them into the next
    /// (its forces are the step's two ends' mean), so the energy guard credits a step
    /// with the work of that mean (`group.rs`).
    pub(super) previous_load: Vec<DVec3>,
    pub(super) previous_torque: Vec<DVec3>,
    /// Each dashpot's force and torque at the end of the last substep (node, N, N·m): in
    /// the accelerations Newmark carries, so in the work its next step's mean does.
    pub(super) dashpot_end: Vec<(usize, DVec3, DVec3)>,
    /// Each node's rest inertia inverted (zero where it has none): its angular velocity from
    /// its momentum, R·I⁻¹·Rᵀ·L, without inverting the turned inertia every step.
    pub(super) rest_inverse: Vec<DMat3>,
    /// Check every Newmark step for energy gain and redo it with BDF2 if it gained (default
    /// on; see `step.rs`).
    pub energy_guard: bool,
    /// Steps the energy guard redid with BDF2 so far.
    pub fallbacks: u32,
    /// Members the last steps found compressed enough to split (`buckling.rs`).
    pub(super) wanted: Vec<usize>,
    pub(super) snapshot: Snapshot,
    pub(super) work: Work,
}

/// Per-substep targets and scratch space.
#[derive(Clone, Debug, Default)]
pub(super) struct Work {
    pub beta: f64,
    /// The substep (s).
    pub h: f64,
    /// How far ahead of the start the velocity reference x̂ lies (s): 0 for backward Euler,
    /// h/3 for BDF2, h/2 for Newmark. The plastic offsets are extrapolated as far.
    pub tau: f64,
    pub x_hat: Vec<DVec3>,
    pub q_hat: Vec<DQuat>,
    /// Predicted positions x* and orientations q*.
    pub x_star: Vec<DVec3>,
    pub q_star: Vec<DQuat>,
    /// World-frame nodal inertia at q*.
    pub inertia_star: Vec<DMat3>,
    /// The angular momentum the prediction carries to the step's end (world frame); the
    /// minimum adds the step's torques' share, I*·turn/β, and the end velocity is the
    /// momentum's (`finish_substep`).
    pub momentum_star: Vec<DVec3>,
    /// Drag dashpot per node for this step: drag·|v| at its start (N·s/m).
    pub drag_rate: Vec<f64>,
    /// Where each dashpot's node would end at its reference motion (`dashpot.rs`).
    pub dashpot_ref: Vec<(DVec3, DQuat)>,
    /// What the dashpots are taken times this substep (`dashpot.rs`: h/β for Newmark).
    pub dashpot_scale: f64,
    /// Whether the dashpots and added masses were referred to this substep's prediction.
    pub referred: bool,
    /// Coriolis force per node for this step (N).
    pub coriolis: Vec<DVec3>,
    /// The rotating frame's fictitious torque per node for this step (N·m).
    pub frame_torque: Vec<DVec3>,
    /// Element measures at (x̂, q̂), for damping.
    pub s_hat: Vec<[f64; MEASURES]>,
    /// How every element flows plastically during this step (`flow.rs`).
    pub caps: Vec<[super::flow::Flow; MEASURES]>,
    /// The same with their work divided by 1 + γ, as the member's series damper sees them
    /// (`plastic.rs`), found once a step (copied anew at every evaluation, the flows cost a
    /// 5k-member bridge 5 % of its step).
    pub scaled: Vec<[super::flow::Flow; MEASURES]>,
    pub rotations: Vec<DMat3>,
    /// Newton direction (translation blocks, then rotation blocks).
    pub direction: Vec<DVec3>,
    pub trial_x: Vec<DVec3>,
    pub trial_q: Vec<DQuat>,
    /// Elastic energy of the structure's current state (kept up to date by the minimisation).
    pub current_elastic: f64,
    /// Per-element results, summed per node afterwards in a fixed order.
    pub element_energy: Vec<[f64; 2]>,
    pub element_force: Vec<[DVec3; 4]>,
    /// For each block (CSR): the (element, dof group) pairs that act on it, by element.
    pub incidence_ptr: Vec<usize>,
    pub incidence: Vec<(u32, u8)>,
}

impl ImplicitSolver {
    /// Prepares stepping `structure` by `dt` seconds per [`Self::step`], split into
    /// `substeps`, and factorises its matrix.
    ///
    /// # Panics
    /// If the matrix is not positive definite (impossible with positive masses).
    pub fn new(structure: &Structure, scheme: Scheme, dt: f64, substeps: u32) -> Self {
        Self::try_new(structure, scheme, dt, substeps)
            .expect("matrix cannot be made positive definite")
    }

    /// As [`Self::new`], or none if the structure's matrix cannot be made positive definite
    /// (a structure read from a damaged save).
    pub(crate) fn try_new(
        structure: &Structure,
        scheme: Scheme,
        dt: f64,
        substeps: u32,
    ) -> Option<Self> {
        let n = structure.node_count();
        let mut solver = Self {
            scheme,
            substeps,
            h: dt / f64::from(substeps),
            max_iterations: 500,
            tolerance: 3.0e-7,
            last_iterations: 0,
            iterations_total: 0,
            timings: Timings::default(),
            last_yielding: super::Yielding::default(),
            refactoring: Refactoring::WhenSlow,
            factorisations: 0,
            factor_orientation: vec![DQuat::IDENTITY; n],
            factor_current: false,
            factor_position: Vec::new(),
            factor_broken: Vec::new(),
            turns: vec![DMat3::IDENTITY; n],
            contact_blocks: Vec::new(),
            chol: Cholesky::new(structure.symbolic()),
            unit_columns: Vec::new(),
            previous_x: structure.position.clone(),
            previous_v: structure.velocity.clone(),
            previous_q: structure.orientation.clone(),
            previous_w: structure.angular_velocity.clone(),
            acceleration: vec![DVec3::ZERO; n],
            angular_acceleration: vec![DVec3::ZERO; n],
            previous_load: structure.load.clone(),
            previous_torque: structure.torque.clone(),
            dashpot_end: Vec::new(),
            rest_inverse: structure
                .inertia
                .iter()
                .map(|m| {
                    if m.determinant() > 0.0 {
                        m.inverse()
                    } else {
                        DMat3::ZERO
                    }
                })
                .collect(),
            energy_guard: true,
            fallbacks: 0,
            snapshot: Snapshot::default(),
            wanted: Vec::new(),
            work: Work {
                x_hat: vec![DVec3::ZERO; n],
                q_hat: vec![DQuat::IDENTITY; n],
                x_star: vec![DVec3::ZERO; n],
                q_star: vec![DQuat::IDENTITY; n],
                inertia_star: vec![DMat3::ZERO; n],
                momentum_star: vec![DVec3::ZERO; n],
                drag_rate: vec![0.0; n],
                dashpot_ref: Vec::new(),
                dashpot_scale: 1.0,
                referred: false,
                coriolis: vec![DVec3::ZERO; n],
                frame_torque: vec![DVec3::ZERO; n],
                s_hat: vec![[0.0; MEASURES]; structure.element_count()],
                caps: vec![[super::flow::Flow::NEVER; MEASURES]; structure.element_count()],
                scaled: vec![[super::flow::Flow::NEVER; MEASURES]; structure.element_count()],
                rotations: vec![DMat3::IDENTITY; n],
                direction: vec![DVec3::ZERO; 2 * n],
                trial_x: vec![DVec3::ZERO; n],
                trial_q: vec![DQuat::IDENTITY; n],
                element_energy: vec![[0.0; 2]; structure.element_count()],
                element_force: vec![[DVec3::ZERO; 4]; structure.element_count()],
                ..Work::default()
            },
        };
        solver.work.beta = solver.beta();
        (solver.work.incidence_ptr, solver.work.incidence) = structure.incidence();
        // Linearise at the shape the structure starts in (often a deformed equilibrium).
        if !solver.try_factor(structure, true) {
            return None;
        }
        solver.factorisations = 0;
        solver.initial_accelerations(structure);
        Some(solver)
    }

    pub(super) fn beta(&self) -> f64 {
        self.beta_for(self.scheme)
    }

    pub(super) fn beta_for(&self, scheme: Scheme) -> f64 {
        match scheme {
            Scheme::BackwardEuler => self.h,
            Scheme::Bdf2 => 2.0 * self.h / 3.0,
            Scheme::Newmark => 0.5 * self.h,
        }
    }

    /// Forgets the history of earlier steps (BDF2 and Newmark use it): after the velocities
    /// changed abruptly (an impact, or set by hand), the motion restarts from the current
    /// state as if it had been moving uniformly, with accelerations from the current forces.
    pub fn reset_history(&mut self, s: &Structure) {
        let h = self.h;
        for i in 0..s.node_count() {
            self.previous_x[i] = s.position[i] - s.velocity[i] * h;
            self.previous_v[i] = s.velocity[i];
            self.previous_q[i] = crate::so3::exp(-s.angular_velocity[i] * h) * s.orientation[i];
            self.previous_w[i] = s.angular_velocity[i];
        }
        self.initial_accelerations(s);
    }

    /// a = M⁻¹·f and α = J⁻¹·τ in the current state, without contact forces (Newmark needs
    /// them from the start, and again after an impact; a
    /// structure released under gravity starts with a = g, not 0).
    pub(crate) fn initial_accelerations(&mut self, s: &Structure) {
        let n = s.node_count();
        // The accelerations now are of the loads now.
        self.previous_load.clone_from(&s.load);
        self.previous_torque.clone_from(&s.torque);
        let w = &mut self.work;
        w.x_star.clone_from(&s.position);
        w.q_star.clone_from(&s.orientation);
        for i in 0..n {
            let r = DMat3::from_quat(s.orientation[i]);
            w.inertia_star[i] = r * s.inertia[i] * r.transpose();
            w.rotations[i] = r;
        }
        for (e, s_hat) in s.elements.iter().zip(&mut w.s_hat) {
            let (xa, xb) = (s.position[e.a], s.position[e.b]);
            *s_hat = e.measures(xa, w.rotations[e.a], xb, w.rotations[e.b]);
        }
        w.set_coriolis(s, None);
        // With x = x* the residual is β²·f (no inertia term, no damping).
        // (Nor dashpots: at the motion they were linearised at they exert nothing.)
        w.dashpot_ref.clear();
        w.referred = false;
        w.residual(s);
        let scale = 1.0 / (w.beta * w.beta);
        for i in 0..n {
            self.acceleration[i] = w.direction[i] * (scale / s.mass[i]);
            let inertia = w.inertia_star[i] + DMat3::from_diagonal(DVec3::ONE - s.free_rotation[i]);
            self.angular_acceleration[i] = inertia.inverse() * (w.direction[n + i] * scale);
        }
    }

    /// The (sub)step length h (s).
    pub fn time_step(&self) -> f64 {
        self.h
    }

    /// Changes the time step to `dt` (split into the solver's substeps) and re-linearises.
    /// The history is rebuilt for the new step from the current velocities and the last
    /// accelerations (x₋ = x − h·v + h²/2·a), so BDF2 continues a decelerating motion
    /// instead of restarting it as uniform (which would make or lose energy).
    pub fn set_time_step(&mut self, structure: &Structure, dt: f64) {
        let h = dt / f64::from(self.substeps);
        #[allow(clippy::float_cmp)] // the very same step: nothing to do
        if h == self.h {
            return;
        }
        self.h = h;
        self.work.beta = self.beta();
        self.refactor(structure);
        let s = structure;
        for i in 0..s.node_count() {
            let (a, alpha) = (self.acceleration[i], self.angular_acceleration[i]);
            self.previous_x[i] = s.position[i] - s.velocity[i] * h + a * (0.5 * h * h);
            self.previous_v[i] = s.velocity[i] - a * h;
            let back = -s.angular_velocity[i] * h + alpha * (0.5 * h * h);
            self.previous_q[i] = crate::so3::exp(back) * s.orientation[i];
            self.previous_w[i] = s.angular_velocity[i] - alpha * h;
        }
    }
}
