//! F2 candidate C (ADR-0004): implicit time stepping with a sparse direct solve.
//!
//! Each substep starts with one linear system for the velocity change Δ:
//!
//! (M + β²·K)·Δ = β·f(x*),  with x* = x̂ + β·v̂ the predicted position,
//!
//! then v = v̂ + Δ and x = x̂ + β·v. K is the stiffness matrix of all members (EA/L along
//! each member). This is one Newton iteration of the implicit scheme; the residual f(x*) uses
//! exact member lengths, so the static equilibrium it settles to is exact. Further iterations
//! with a line search (`newton.rs`) keep violent motion stable.
//!
//! - Backward Euler: β = h, x̂ = x, v̂ = v. First order, damps fast modes strongly.
//! - BDF2: β = 2h/3, x̂ = (4x − x₋₁)/3, v̂ = (4v − v₋₁)/3. Second order, much less damping.
//!
//! The matrix only changes when the structure or the step changes, so its factorisation is
//! computed once and reused for every substep. K belongs to the rest shape, though, and a
//! body that rotates as a whole would make it wrong; see [`Linearisation`] for the options.

use super::*;
use crate::rotation::ShapeMatcher;
use crate::sparse::{Cholesky, SymbolicCholesky};
use creeer_math::glam::{DMat3, DQuat};

/// Implicit integration scheme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scheme {
    /// Backward (implicit) Euler, first order.
    BackwardEuler,
    /// Second-order backward differentiation formula.
    Bdf2,
}

/// Which stiffness matrix the implicit step linearises with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Linearisation {
    /// K of the rest shape, factorised once. Wrong for a body that has rotated.
    RestFrame,
    /// K of the rest shape, factorised once, applied in the body's own frame: forces are
    /// rotated by the overall rotation R into the rest frame, solved, and rotated back
    /// ((M + β²·R·K·Rᵀ) = R·(M + β²·K)·Rᵀ because every node's mass is the same on all axes).
    /// Supported (anchored) structures cannot rotate as a whole and keep R = I.
    BodyFrame,
    /// K with the current member directions, refactorised every substep. Exact but slow.
    Corotational,
}

impl Truss {
    fn edges(&self) -> Vec<(usize, usize)> {
        self.beams.iter().map(|b| (b.a, b.b)).collect()
    }

    /// Fills `diag`/`off` with `mass_scale·M + stiffness_scale·K`, using member directions
    /// at `positions`, and replaces supported axes by identity rows (Δ = 0 there).
    fn assemble(
        &self,
        positions: &[DVec3],
        mass_scale: f64,
        stiffness_scale: f64,
        diag: &mut [DMat3],
        off: &mut [DMat3],
    ) {
        for (d, m) in diag.iter_mut().zip(&self.mass) {
            *d = DMat3::IDENTITY * (mass_scale * m);
        }
        for (beam, block) in self.beams.iter().zip(off.iter_mut()) {
            let e = (positions[beam.a] - positions[beam.b]).normalize();
            let k =
                DMat3::from_cols(e * e.x, e * e.y, e * e.z) * (stiffness_scale / beam.compliance);
            diag[beam.a] += k;
            diag[beam.b] += k;
            *block = -k;
        }
        let project = |node: usize| DMat3::from_diagonal(self.free_mask(node));
        for (i, d) in diag.iter_mut().enumerate() {
            let p = project(i);
            *d = p * *d * p + (DMat3::IDENTITY - p);
        }
        for (beam, block) in self.beams.iter().zip(off.iter_mut()) {
            *block = project(beam.a) * *block * project(beam.b);
        }
    }
}

/// Exact linear static displacement of `truss` under self-weight, solved with the sparse
/// block Cholesky factorisation (production path for the quasi-static FEM of ADR-0002).
///
/// # Panics
/// If the structure is a mechanism (not enough members or supports to carry load).
pub fn sparse_static_displacement(truss: &Truss) -> Vec<DVec3> {
    let n = truss.node_count();
    let (mut diag, mut off) = (vec![DMat3::ZERO; n], vec![DMat3::ZERO; truss.beam_count()]);
    truss.assemble(&truss.rest, 0.0, 1.0, &mut diag, &mut off);
    let mut chol = Cholesky::new(SymbolicCholesky::new(&truss.rest, &truss.edges()));
    chol.factor(&diag, &off).expect("structure is a mechanism");
    let mut u = vec![DVec3::ZERO; n];
    truss.forces(&truss.rest, &mut u);
    chol.solve(&mut u);
    u
}

/// A truss stepped with an implicit scheme.
#[derive(Clone, Debug)]
pub struct ImplicitTruss {
    /// The structure and its current state.
    pub truss: Truss,
    scheme: Scheme,
    substeps: u32,
    h: f64,
    /// How the step is linearised (default: [`Linearisation::BodyFrame`]).
    pub linearisation: Linearisation,
    /// Newton iterations per substep at most (default 4). One iteration is the classic
    /// linearly implicit step; more make violent motion robust (see `newton.rs`).
    pub max_iterations: u32,
    /// A substep stops iterating once no node moves more than this (m).
    pub tolerance: f64,
    /// Newton iterations used by the last tick (summed over its substeps).
    pub last_iterations: u32,
    /// Overall rotation of the body (for [`Linearisation::BodyFrame`]).
    rotation: DQuat,
    matcher: ShapeMatcher,
    anchored: bool,
    chol: Cholesky,
    previous_x: Vec<DVec3>,
    previous_v: Vec<DVec3>,
    /// x̂ of the current substep.
    x_hat: Vec<DVec3>,
    /// Predicted position x* = x̂ + β·v̂ of the current substep.
    predicted: Vec<DVec3>,
    rhs: Vec<DVec3>,
    trial: Vec<DVec3>,
    diag: Vec<DMat3>,
    off: Vec<DMat3>,
}

impl ImplicitTruss {
    /// Prepares `truss` for ticks of `dt` seconds split into `substeps` implicit steps,
    /// factoring the matrix once from the rest shape.
    ///
    /// # Panics
    /// If the system matrix is not positive definite (impossible with positive masses).
    pub fn new(truss: Truss, scheme: Scheme, dt: f64, substeps: u32) -> Self {
        let n = truss.node_count();
        let sym = SymbolicCholesky::new(&truss.rest, &truss.edges());
        let mut this = Self {
            scheme,
            substeps,
            h: dt / f64::from(substeps),
            linearisation: Linearisation::BodyFrame,
            max_iterations: 4,
            tolerance: 1.0e-6,
            last_iterations: 0,
            rotation: DQuat::IDENTITY,
            matcher: ShapeMatcher::new(&truss.rest, &truss.mass),
            anchored: (0..n).any(|i| truss.free_mask(i) != DVec3::ONE),
            chol: Cholesky::new(sym),
            previous_x: truss.position.clone(),
            previous_v: truss.velocity.clone(),
            x_hat: vec![DVec3::ZERO; n],
            predicted: vec![DVec3::ZERO; n],
            rhs: vec![DVec3::ZERO; n],
            trial: vec![DVec3::ZERO; n],
            diag: vec![DMat3::ZERO; n],
            off: vec![DMat3::ZERO; truss.beam_count()],
            truss,
        };
        let rest = this.truss.rest.clone();
        this.refactor(&rest);
        this
    }

    fn beta(&self) -> f64 {
        match self.scheme {
            Scheme::BackwardEuler => self.h,
            Scheme::Bdf2 => 2.0 * self.h / 3.0,
        }
    }

    /// Recomputes the factorisation with member directions at `positions`.
    fn refactor(&mut self, positions: &[DVec3]) {
        let beta = self.beta();
        self.truss
            .assemble(positions, 1.0, beta * beta, &mut self.diag, &mut self.off);
        self.chol
            .factor(&self.diag, &self.off)
            .expect("M + β²K is positive definite for positive masses");
    }

    /// Advances one tick.
    pub fn step(&mut self) {
        self.last_iterations = 0;
        for _ in 0..self.substeps {
            self.substep();
        }
    }
}

mod newton;
