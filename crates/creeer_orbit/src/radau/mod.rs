//! IAS15: an adaptive 15th-order Gauss–Radau integrator for gravitating particles
//! (Everhart 1985; Rein & Spiegel 2015, MNRAS 446, 1424; step control after Pham, Rein &
//! Spiegel 2024, OJAp 7).
//!
//! Every step fits the acceleration over the step with a polynomial of degree 7 through
//! eight substeps (the Gauss–Radau nodes), iterating predictor and corrector until the fit
//! stops changing, then integrates the polynomial exactly. Its errors stay below the
//! rounding of f64 at the default ε = 10⁻⁹, so energy errors grow only like a random walk of
//! rounding (Brouwer's law) — no secular drift, without needing a symplectic splitting.
//! Close encounters (an asteroid grazing a moon) are resolved by shrinking the step;
//! symplectic Wisdom–Holman maps would fail there (ADR-0007).
//!
//! Positions and velocities are summed with Kahan compensation, so increments far below the
//! last bit of a planet's coordinates (a thruster pushing a moon) accumulate exactly instead
//! of being rounded away.

pub mod coeffs;
mod fit;
mod segment;
#[cfg(test)]
mod tests;

use coeffs::{H, binomial, coefficients};
use creeer_math::glam::DVec3;
pub use segment::Segment;
use segment::{position_increment, velocity_increment};

/// Accelerations of all particles of a system.
pub trait Forces {
    /// Writes the acceleration of every particle at time `t` (s) for positions `x` and
    /// velocities `v` into `a`.
    fn accelerations(&mut self, t: f64, x: &[DVec3], v: &[DVec3], a: &mut [DVec3]);

    /// Whether the accelerations depend on the velocities (then they are predicted at every
    /// substep, which costs a little).
    fn needs_velocities(&self) -> bool {
        false
    }
}

/// Accuracy parameter: the step is (ε·7!)^{1/7} ≈ 0.175 of the shortest dynamical time.
pub const DEFAULT_EPSILON: f64 = 1e-9;
/// A step is redone when the next one would be shorter than this fraction of it.
const SAFETY: f64 = 0.25;
/// Most predictor–corrector iterations per step.
const MAX_ITERATIONS: usize = 12;
/// Most redone attempts of one step (each at most a quarter of the last: 4⁶⁴ ≈ 10³⁸). Only
/// reached by a degenerate system (two point masses on top of each other).
const MAX_REJECTIONS: usize = 64;
/// A fit stretched over a step more than this many times as long is no prediction at all
/// (the coefficients of a very short step are rounding noise, and the stretch multiplies
/// bₘ by q^{m+1}): start the fit from zero instead.
const MAX_STRETCH: f64 = 1.0 / SAFETY;

/// Polynomial coefficients of one particle's acceleration over a step: a(h) = a₀ + Σ bₘ·h^{m+1}.
pub type Poly = [DVec3; 7];

/// 1/((m+2)(m+3)): position weights of bₘ.
const X_WEIGHT: [f64; 7] = [
    1.0 / 6.0,
    1.0 / 12.0,
    1.0 / 20.0,
    1.0 / 30.0,
    1.0 / 42.0,
    1.0 / 56.0,
    1.0 / 72.0,
];
/// 1/(m+2): velocity weights of bₘ.
const V_WEIGHT: [f64; 7] = [
    1.0 / 2.0,
    1.0 / 3.0,
    1.0 / 4.0,
    1.0 / 5.0,
    1.0 / 6.0,
    1.0 / 7.0,
    1.0 / 8.0,
];

/// The integrator's state: particles, the current step size and the fit carried between
/// steps.
#[derive(Clone, Debug)]
pub struct Radau {
    /// Time (s).
    pub t: f64,
    /// Positions (m).
    pub x: Vec<DVec3>,
    /// Velocities (m/s).
    pub v: Vec<DVec3>,
    /// Kahan compensations of x and v (true value = stored − compensation).
    comp_x: Vec<DVec3>,
    comp_v: Vec<DVec3>,
    /// Accelerations at `t` (valid while `fresh`).
    a0: Vec<DVec3>,
    fresh: bool,
    b: Vec<Poly>,
    g: Vec<Poly>,
    /// The prediction b started the last step from (refines the next prediction).
    e: Vec<Poly>,
    /// The converged fit of the last finished step (its dense output).
    last: Vec<Poly>,
    /// The step length b and e are scaled for (s).
    fitted_dt: f64,
    /// Step size to try next (s).
    pub dt: f64,
    /// Accuracy parameter.
    pub epsilon: f64,
    /// Shortest step: accepted even if the error control asks for less (s).
    pub min_dt: f64,
    /// Force evaluations so far (for benchmarks).
    pub evaluations: u64,
    scratch_x: Vec<DVec3>,
    scratch_v: Vec<DVec3>,
    scratch_a: Vec<DVec3>,
}

impl Radau {
    /// Particles at time `t` with a first step to try of `dt` seconds.
    pub fn new(t: f64, x: Vec<DVec3>, v: Vec<DVec3>, dt: f64) -> Self {
        assert_eq!(x.len(), v.len(), "one velocity per position");
        let n = x.len();
        Self {
            t,
            x,
            v,
            comp_x: vec![DVec3::ZERO; n],
            comp_v: vec![DVec3::ZERO; n],
            a0: vec![DVec3::ZERO; n],
            fresh: false,
            b: vec![[DVec3::ZERO; 7]; n],
            g: vec![[DVec3::ZERO; 7]; n],
            e: vec![[DVec3::ZERO; 7]; n],
            last: vec![[DVec3::ZERO; 7]; n],
            fitted_dt: dt,
            dt,
            epsilon: DEFAULT_EPSILON,
            min_dt: 0.0,
            evaluations: 0,
            scratch_x: vec![DVec3::ZERO; n],
            scratch_v: vec![DVec3::ZERO; n],
            scratch_a: vec![DVec3::ZERO; n],
        }
    }

    /// Number of particles.
    pub fn len(&self) -> usize {
        self.x.len()
    }

    /// Whether there are no particles.
    pub fn is_empty(&self) -> bool {
        self.x.is_empty()
    }

    /// Replaces a particle's state (a kick, a collision): forgets the fitted accelerations.
    pub fn set_state(&mut self, i: usize, x: DVec3, v: DVec3) {
        self.x[i] = x;
        self.v[i] = v;
        self.comp_x[i] = DVec3::ZERO;
        self.comp_v[i] = DVec3::ZERO;
        self.reset();
    }

    /// The fitted acceleration polynomial of the last finished step of particle `i`: with
    /// the state at its start it gives the motion anywhere inside the step ([`Segment`]).
    pub fn last_fit(&self, i: usize) -> &Poly {
        &self.last[i]
    }

    /// Kahan compensations of particle `i`'s position and velocity: with them, a state can be
    /// stored and restored without losing the increments below the last bit.
    pub fn compensation(&self, i: usize) -> (DVec3, DVec3) {
        (self.comp_x[i], self.comp_v[i])
    }

    /// Puts every particle back into a stored state at time `t`: (x, v, compensation of x,
    /// compensation of v) per particle.
    pub fn restore(&mut self, t: f64, states: impl IntoIterator<Item = [DVec3; 4]>) {
        let mut count = 0;
        for (i, [x, v, cx, cv]) in states.into_iter().enumerate() {
            self.x[i] = x;
            self.v[i] = v;
            self.comp_x[i] = cx;
            self.comp_v[i] = cv;
            count += 1;
        }
        assert_eq!(count, self.len(), "one state per particle");
        self.t = t;
        self.reset();
    }

    /// Forgets the fitted accelerations: call when the forces change discontinuously.
    pub fn reset(&mut self) {
        self.fresh = false;
        for p in self.b.iter_mut().chain(&mut self.g).chain(&mut self.e) {
            *p = [DVec3::ZERO; 7];
        }
    }

    /// The accelerations at the current time.
    pub fn accelerations(&mut self, forces: &mut impl Forces) -> &[DVec3] {
        self.ensure_a0(forces);
        &self.a0
    }

    fn ensure_a0(&mut self, forces: &mut impl Forces) {
        if !self.fresh {
            forces.accelerations(self.t, &self.x, &self.v, &mut self.a0);
            self.evaluations += 1;
            self.fresh = true;
        }
    }

    /// Advances by one step, ending exactly at time `until` if the error control would go
    /// further (pass `f64::INFINITY` for a free step). Returns the step taken (s).
    pub fn step(&mut self, forces: &mut impl Forces, until: f64) -> f64 {
        self.ensure_a0(forces);
        let natural = self.dt;
        let mut rejections = 0;
        loop {
            let clipped = self.dt >= until - self.t;
            let dt = if clipped { until - self.t } else { self.dt };
            #[allow(clippy::float_cmp)] // any other length needs the rescale
            if dt != self.fitted_dt {
                self.rescale_fit(dt / self.fitted_dt);
                self.fitted_dt = dt;
            }
            self.converge(forces, dt);
            let dt_new = self.next_step(dt);
            if dt_new < SAFETY * dt && dt > self.min_dt && rejections < MAX_REJECTIONS {
                // Too coarse: redo with the shorter step.
                self.dt = dt_new.max(self.min_dt);
                rejections += 1;
                continue;
            }
            // Grow by at most 1/SAFETY over the natural step (a step cut short by `until`
            // does not hold the following ones back).
            self.finish(dt, dt_new.min(natural.max(dt) / SAFETY), clipped);
            if clipped {
                self.t = until; // exactly, whatever t + dt rounded to
            }
            forces.accelerations(self.t, &self.x, &self.v, &mut self.a0);
            self.evaluations += 1;
            self.fresh = true;
            return dt;
        }
    }

    /// Iterates predictor and corrector over the eight substeps of a step `dt` until the fit
    /// of the accelerations stops changing.
    #[allow(clippy::needless_range_loop)] // s and j index several tables at once
    fn converge(&mut self, forces: &mut impl Forces, dt: f64) {
        let co = coefficients();
        self.g_from_b();
        let mut error_last = f64::INFINITY;
        for iteration in 0..MAX_ITERATIONS {
            let mut max_change = 0.0f64;
            let mut max_accel = 0.0f64;
            for s in 1..8 {
                self.predict(H[s], dt, forces.needs_velocities());
                forces.accelerations(
                    self.t + H[s] * dt,
                    &self.scratch_x,
                    &self.scratch_v,
                    &mut self.scratch_a,
                );
                self.evaluations += 1;
                let r = &co.r[s];
                for i in 0..self.len() {
                    // Divided differences give the Newton coefficient gₛ.
                    let mut gs = (self.scratch_a[i] - self.a0[i]) * r[0];
                    for j in 1..s {
                        gs = (gs - self.g[i][j - 1]) * r[j];
                    }
                    let change = gs - self.g[i][s - 1];
                    self.g[i][s - 1] = gs;
                    for m in 0..s {
                        self.b[i][m] += change * co.c[s][m + 1];
                    }
                    if s == 7 {
                        max_change = max_change.max(change.abs().max_element());
                        max_accel = max_accel.max(self.scratch_a[i].abs().max_element());
                    }
                }
            }
            let error = if max_accel > 0.0 {
                max_change / max_accel
            } else {
                0.0
            };
            // Converged to rounding, or the corrector stopped improving.
            if error < 1e-16 || (iteration > 1 && error >= error_last) {
                break;
            }
            error_last = error;
        }
    }

    /// Positions (and velocities) at substep `h` of a step `dt` into the scratch arrays.
    fn predict(&mut self, h: f64, dt: f64, velocities: bool) {
        let hd = h * dt;
        for i in 0..self.len() {
            let b = &self.b[i];
            let increment = position_increment(b, self.v[i], self.a0[i], h, hd);
            self.scratch_x[i] = self.x[i] + (increment - self.comp_x[i]);
            if velocities {
                let increment = velocity_increment(b, self.a0[i], h, hd);
                self.scratch_v[i] = self.v[i] + (increment - self.comp_v[i]);
            } else {
                self.scratch_v[i] = self.v[i];
            }
        }
    }

    /// Commits the step: new positions and velocities (compensated), the prediction of the
    /// next step's fit, and its size. A step cut short (`clipped`) is likely followed by one
    /// as short (game ticks), so its fit is carried over unstretched.
    fn finish(&mut self, dt: f64, dt_new: f64, clipped: bool) {
        for i in 0..self.len() {
            let b = &self.b[i];
            let (mut px, mut pv) = (DVec3::ZERO, DVec3::ZERO);
            for (m, bm) in b.iter().enumerate() {
                px += *bm * X_WEIGHT[m];
                pv += *bm * V_WEIGHT[m];
            }
            let dx = self.v[i] * dt + (self.a0[i] * 0.5 + px) * (dt * dt);
            let dv = (self.a0[i] + pv) * dt;
            kahan(&mut self.x[i], &mut self.comp_x[i], dx);
            kahan(&mut self.v[i], &mut self.comp_v[i], dv);
        }
        self.t += dt;
        self.last.clone_from(&self.b);
        // The next fit starts from this one, expanded about the new start:
        // a(1 + q·h') = a(1) + Σₘ h'^{m+1}·q^{m+1}·Σ_{j≥m} C(j+1, m+1)·bⱼ.
        let fitted = if clipped { dt } else { dt_new };
        let q = fitted / dt;
        for i in 0..self.len() {
            let b = self.b[i];
            let mut predicted = [DVec3::ZERO; 7];
            let mut qk = q;
            for (m, p) in predicted.iter_mut().enumerate() {
                for (j, bj) in b.iter().enumerate().skip(m) {
                    *p += *bj * binomial(j + 1, m + 1);
                }
                *p *= qk;
                qk *= q;
            }
            for m in 0..7 {
                // Keep the last correction to the prediction (Rein & Spiegel 2015, §2.6).
                let correction = b[m] - self.e[i][m];
                self.e[i][m] = predicted[m];
                self.b[i][m] = predicted[m] + correction;
            }
        }
        self.dt = dt_new;
        self.fitted_dt = fitted;
    }

    /// Integrates until `t_end`, calling `each` after every step.
    pub fn integrate_to(
        &mut self,
        forces: &mut impl Forces,
        t_end: f64,
        mut each: impl FnMut(&Self),
    ) {
        while self.t < t_end {
            self.step(forces, t_end);
            each(self);
        }
    }
}

/// Kahan-compensated `sum += increment` (true sum = sum − compensation).
#[inline]
fn kahan(sum: &mut DVec3, compensation: &mut DVec3, increment: DVec3) {
    let y = increment - *compensation;
    let t = *sum + y;
    *compensation = (t - *sum) - y;
    *sum = t;
}
