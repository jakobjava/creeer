//! Dynamic references for the F2 solver comparison (ADR-0004): forces, the exact first
//! vibration frequency, and measuring frequency and damping from a simulated response.
//!
//! The test load case: the bridge starts undeformed and at rest, and gravity is switched on.
//! It then swings about its static sag, mostly in its first vertical bending mode. A correct
//! solver reproduces that mode's frequency and adds no (or little) artificial damping.

use super::*;
use creeer_math::det;

impl Truss {
    /// Whether each axis of `node` is free to move (1) or held by a support (0).
    pub(super) fn free_mask(&self, node: usize) -> DVec3 {
        DVec3::select(
            self.inv_mass[node].cmpgt(DVec3::ZERO),
            DVec3::ONE,
            DVec3::ZERO,
        )
    }

    /// Forces on every node at `positions`: member forces from the exact (nonlinear) member
    /// lengths plus gravity, with supported axes zeroed.
    pub(super) fn forces(&self, positions: &[DVec3], out: &mut [DVec3]) {
        for (f, m) in out.iter_mut().zip(&self.mass) {
            *f = GRAVITY * *m;
        }
        for beam in &self.beams {
            let delta = positions[beam.a] - positions[beam.b];
            let length = delta.length();
            // Tension (positive) = stiffness · elongation, with stiffness EA/L = 1/compliance.
            let tension = (length - beam.rest_length) / beam.compliance;
            let pull = delta * (tension / length);
            out[beam.a] -= pull;
            out[beam.b] += pull;
        }
        for (i, f) in out.iter_mut().enumerate() {
            *f *= self.free_mask(i);
        }
    }

    /// Largest member strain |L/L₀ − 1| (steel yields at about 0.1 %).
    pub fn max_strain(&self) -> f64 {
        self.beams
            .iter()
            .map(|b| {
                ((self.position[b.a] - self.position[b.b]).length() / b.rest_length - 1.0).abs()
            })
            .fold(0.0, f64::max)
    }

    /// Total mechanical energy (J): kinetic + elastic + gravitational (zero at the rest shape).
    pub fn energy(&self) -> f64 {
        let kinetic: f64 = self
            .velocity
            .iter()
            .zip(&self.mass)
            .map(|(v, m)| 0.5 * m * v.length_squared())
            .sum();
        let elastic: f64 = self
            .beams
            .iter()
            .map(|b| {
                let stretch = (self.position[b.a] - self.position[b.b]).length() - b.rest_length;
                0.5 * stretch * stretch / b.compliance
            })
            .sum();
        let gravity: f64 = self
            .position
            .iter()
            .zip(&self.rest)
            .zip(&self.mass)
            .map(|((x, x0), m)| -m * GRAVITY.dot(*x - *x0))
            .sum();
        kinetic + elastic + gravity
    }

    /// Sets every node's velocity to that of a rigid rotation `omega` (rad/s) about the
    /// centre of mass.
    pub fn spin(&mut self, omega: DVec3) {
        let total: f64 = self.mass.iter().sum();
        let centre = self
            .position
            .iter()
            .zip(&self.mass)
            .fold(DVec3::ZERO, |c, (p, m)| c + *p * *m)
            / total;
        for (v, p) in self.velocity.iter_mut().zip(&self.position) {
            *v = omega.cross(*p - centre);
        }
    }
}

/// Frequency and damping of an oscillation, measured from samples.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Oscillation {
    /// Mean frequency over the recorded cycles (Hz).
    pub frequency: f64,
    /// Equivalent viscous damping ratio ζ from the decay of the peaks (0 = no damping).
    pub damping_ratio: f64,
    /// Number of full cycles the measurement used.
    pub cycles: usize,
}

/// Measures the oscillation of `samples` (taken every `interval` seconds) about
/// `equilibrium`. Returns `None` if fewer than two full cycles were recorded.
pub fn oscillation(samples: &[f64], interval: f64, equilibrium: f64) -> Option<Oscillation> {
    let offset: Vec<f64> = samples.iter().map(|s| s - equilibrium).collect();
    // Upward zero crossings, linearly interpolated between samples.
    let mut crossings = Vec::new();
    for (i, pair) in offset.windows(2).enumerate() {
        if pair[0] < 0.0 && pair[1] >= 0.0 {
            #[allow(clippy::cast_precision_loss)]
            let t = (i as f64 + pair[0] / (pair[0] - pair[1])) * interval;
            crossings.push((t, i + 1));
        }
    }
    if crossings.len() < 3 {
        return None;
    }
    let cycles = crossings.len() - 1;
    let (first, last) = (crossings[0], crossings[cycles]);
    #[allow(clippy::cast_precision_loss)]
    let frequency = cycles as f64 / (last.0 - first.0);
    // Positive peak within each cycle; log decrement between the first and last one.
    let peak = |from: usize, to: usize| offset[from..to].iter().copied().fold(0.0, f64::max);
    let first_peak = peak(crossings[0].1, crossings[1].1);
    let last_peak = peak(crossings[cycles - 1].1, crossings[cycles].1);
    #[allow(clippy::cast_precision_loss)]
    let decrement = det::ln(first_peak / last_peak) / (cycles - 1).max(1) as f64;
    let two_pi = 2.0 * std::f64::consts::PI;
    let damping_ratio = decrement / det::hypot(two_pi, decrement);
    Some(Oscillation {
        frequency,
        damping_ratio,
        cycles,
    })
}

/// First vertical vibration frequency (Hz) of the reference bridge by Rayleigh's method,
/// using the exact static deflection shape u: ω² = Σ m·g·u / Σ m·|u|².
///
/// For the fundamental mode this estimate is an upper bound that is typically within 1 %
/// of the exact value; it is an independent check of the simulated reference below.
pub fn rayleigh_frequency(spec: BridgeSpec) -> f64 {
    let truss = Truss::box_bridge(DVec3::ZERO, spec);
    let u = linear_static_displacement(spec);
    let (work, kinetic) = truss
        .mass
        .iter()
        .zip(&u)
        .fold((0.0, 0.0), |(w, k), (m, u)| {
            (w + m * GRAVITY.dot(*u), k + m * u.length_squared())
        });
    (work / kinetic).sqrt() / (2.0 * std::f64::consts::PI)
}

/// Reference response for the dynamic test: explicit symplectic Euler with a time step far
/// below its stability limit (h = 1/60 000 s, about 10× smaller than needed), so its error
/// for the slow first mode is negligible. Returns the midspan sag every 1/60 s for `ticks`
/// ticks. Far too slow for the game; only used as ground truth.
pub fn explicit_reference_response(spec: BridgeSpec, ticks: usize) -> Vec<f64> {
    const STEPS_PER_TICK: usize = 1000;
    let h = 1.0 / 60.0 / 1000.0;
    let mut truss = Truss::box_bridge(DVec3::ZERO, spec);
    let mut force = vec![DVec3::ZERO; truss.node_count()];
    let mut samples = Vec::with_capacity(ticks);
    for _ in 0..ticks {
        for _ in 0..STEPS_PER_TICK {
            truss.forces(&truss.position, &mut force);
            for (((x, v), f), m) in truss
                .position
                .iter_mut()
                .zip(&mut truss.velocity)
                .zip(&force)
                .zip(&truss.mass)
            {
                *v += *f * (h / m);
                *x += *v * h;
            }
        }
        samples.push(truss.midspan_sag());
    }
    samples
}
