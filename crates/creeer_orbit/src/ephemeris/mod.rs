//! The major bodies' trajectories: integrated together (mutual gravity plus pushes) and
//! stored step by step, so any part of the simulation can ask where a body is at any time
//! in the covered span (ADR-0007).
//!
//! The stored span is the **truth up to now and the forecast after it**. Test particles
//! (ships, small asteroids) are propagated against it; the map view draws predictions from
//! it. A new push makes the forecast after its start wrong: the samples after it are dropped
//! and the bodies are integrated again from there, starting from a stored state with its
//! Kahan compensation, so nothing is lost by stopping and restarting.
//!
//! All bodies share the integrator's steps, so one binary search finds the interval for all
//! of them. Between samples, positions come from the step's own fitted acceleration
//! polynomial ([`Segment`]): the integrator's trajectory itself, not an interpolation of it
//! (quintic Hermite interpolation between the samples was tried first; with steps of T/36
//! it was off by up to 27 m on the Moon's orbit). At push edges two samples share one time
//! (the acceleration jumps).

#[cfg(test)]
mod tests;

use crate::body::Body;
use crate::majors::{self, MajorForces, Push};
use crate::radau::{Forces, Radau, Segment};
use creeer_math::glam::DVec3;

/// Major bodies, their integrator and their stored trajectories.
#[derive(Clone, Debug)]
pub struct Ephemeris {
    bodies: Vec<Body>,
    radau: Radau,
    pushes: Vec<Push>,
    times: Vec<f64>,
    /// `times.len()` rows of `bodies.len()` samples.
    /// Each body's state at the sample with the fit of the step that starts there (zero
    /// until that step is taken).
    samples: Vec<Segment>,
}

impl Ephemeris {
    /// Bodies with positions `x` and velocities `v` (inertial, m and m/s) at time `t0`.
    pub fn new(bodies: Vec<Body>, x: Vec<DVec3>, v: Vec<DVec3>, t0: f64) -> Self {
        assert!(!bodies.is_empty(), "at least one body");
        assert_eq!(bodies.len(), x.len(), "one position per body");
        // First step: a hundredth of the shortest free-fall time between two bodies.
        let mut shortest = f64::INFINITY;
        for i in 0..x.len() {
            for j in i + 1..x.len() {
                let r = (x[j] - x[i]).length();
                shortest = shortest.min((r * r * r / (bodies[i].mu + bodies[j].mu)).sqrt());
            }
        }
        let dt = if shortest.is_finite() {
            0.01 * shortest
        } else {
            3600.0
        };
        let mut this = Self {
            radau: Radau::new(t0, x, v, dt),
            bodies,
            pushes: Vec::new(),
            times: Vec::new(),
            samples: Vec::new(),
        };
        this.record(false);
        this
    }

    /// The bodies.
    pub fn bodies(&self) -> &[Body] {
        &self.bodies
    }

    /// Number of bodies.
    pub fn len(&self) -> usize {
        self.bodies.len()
    }

    /// Whether there are no bodies (never: construction needs one).
    pub fn is_empty(&self) -> bool {
        self.bodies.is_empty()
    }

    /// First time covered (s).
    pub fn start(&self) -> f64 {
        self.times[0]
    }

    /// Last time covered (s).
    pub fn end(&self) -> f64 {
        *self.times.last().expect("never empty")
    }

    /// Number of stored sample times.
    pub fn sample_count(&self) -> usize {
        self.times.len()
    }

    /// Force evaluations so far (for benchmarks).
    pub fn evaluations(&self) -> u64 {
        self.radau.evaluations
    }

    /// The pushes known so far.
    pub fn pushes(&self) -> &[Push] {
        &self.pushes
    }

    /// Integrates until the stored span reaches `t`.
    pub fn extend_to(&mut self, t: f64) {
        let n = self.len();
        while self.end() < t {
            let edge = majors::next_edge(&self.pushes, self.radau.t).unwrap_or(f64::INFINITY);
            let mut forces = MajorForces {
                bodies: &self.bodies,
                pushes: &self.pushes,
                left: false,
            };
            self.radau.step(&mut forces, edge);
            // The step started at the last sample: store its fit there.
            let row = self.times.len() - 1;
            for i in 0..n {
                self.samples[row * n + i].b = *self.radau.last_fit(i);
            }
            #[allow(clippy::float_cmp)] // the step was cut to end exactly there
            if self.radau.t == edge {
                // The acceleration jumps here: the interval before ends with the old one.
                self.record(true);
            }
            self.record(false);
        }
    }

    /// Stores the integrator's current state as a sample (with the acceleration from below
    /// if `left`).
    fn record(&mut self, left: bool) {
        let mut forces = MajorForces {
            bodies: &self.bodies,
            pushes: &self.pushes,
            left,
        };
        let mut a = vec![DVec3::ZERO; self.len()];
        if left {
            forces.accelerations(self.radau.t, &self.radau.x, &self.radau.v, &mut a);
        } else {
            a.copy_from_slice(self.radau.accelerations(&mut forces));
        }
        self.times.push(self.radau.t);
        for (i, a) in a.into_iter().enumerate() {
            let (cx, cv) = self.radau.compensation(i);
            self.samples.push(Segment {
                x: self.radau.x[i],
                v: self.radau.v[i],
                comp: [cx, cv],
                a,
                b: [DVec3::ZERO; 7],
            });
        }
    }

    /// Adds a push. If it starts inside the stored span, the forecast after its start is
    /// dropped and recomputed on the next [`extend_to`](Self::extend_to).
    ///
    /// # Panics
    /// If the push starts before the stored span, or ends before it starts.
    pub fn push(&mut self, push: Push) {
        assert!(push.start >= self.start(), "the past cannot be pushed");
        assert!(push.end >= push.start, "a push ends after it starts");
        assert!(push.body < self.len(), "no such body");
        self.pushes.push(push);
        self.truncate(push.start);
    }

    /// Drops every sample after `t` and puts the integrator back to the last one at or
    /// before it.
    fn truncate(&mut self, t: f64) {
        if t >= self.end() {
            return; // the next steps stop at the new edge by themselves
        }
        let n = self.len();
        let keep = self.times.partition_point(|&s| s <= t);
        let at_t = self.times.partition_point(|&s| s < t);
        // Restart from the last sample before t, or from the first one exactly at t.
        let k = if at_t < keep { at_t } else { keep - 1 };
        let states: Vec<[DVec3; 4]> = self.samples[k * n..(k + 1) * n]
            .iter()
            .map(|s| [s.x, s.v, s.comp[0], s.comp[1]])
            .collect();
        self.radau.restore(self.times[k], states);
        if at_t < keep {
            // A sample lies exactly on the new edge: store it again as a pair, the interval
            // before ending with the acceleration from below.
            self.times.truncate(k);
            self.samples.truncate(k * n);
            self.record(true);
            self.record(false);
        } else {
            self.times.truncate(keep);
            self.samples.truncate(keep * n);
        }
    }

    /// The integrator's velocity of `body` at the last sample, with its Kahan compensation
    /// (true velocity = v − compensation).
    pub fn velocity_with_compensation(&self, body: usize) -> (DVec3, DVec3) {
        (self.radau.v[body], self.radau.compensation(body).1)
    }

    /// Forgets samples before `t` (keeping the interval that contains it).
    pub fn forget_before(&mut self, t: f64) {
        let k = self.times.partition_point(|&s| s <= t).saturating_sub(1);
        if k > 0 {
            self.times.drain(..k);
            self.samples.drain(..k * self.len());
            let start = self.start();
            self.pushes.retain(|p| p.end >= start);
        }
    }

    /// The step containing `t`: its first sample index, length (0 at the very end) and the
    /// fraction s ∈ [0, 1].
    ///
    /// # Panics
    /// If `t` is outside the stored span.
    fn locate(&self, t: f64) -> (usize, f64, f64) {
        assert!(
            t >= self.start() && t <= self.end(),
            "time {t} outside the ephemeris [{}, {}]",
            self.start(),
            self.end()
        );
        let last = self.times.len() - 1;
        #[allow(clippy::float_cmp)] // the very last sample, not a close time
        if t == self.end() {
            return (last, 0.0, 0.0);
        }
        // The last sample at or before t (never the very last one: t < end).
        let k = self.times.partition_point(|&s| s <= t) - 1;
        let (t0, t1) = (self.times[k], self.times[k + 1]);
        (k, t1 - t0, (t - t0) / (t1 - t0))
    }

    /// Position of `body` at time `t` (inside the stored span).
    pub fn position(&self, body: usize, t: f64) -> DVec3 {
        let (k, dt, s) = self.locate(t);
        let segment = &self.samples[k * self.len() + body];
        if dt == 0.0 {
            return segment.x;
        }
        segment.position(dt, s)
    }

    /// Position and velocity of `body` at time `t` (inside the stored span).
    pub fn state(&self, body: usize, t: f64) -> (DVec3, DVec3) {
        let (k, dt, s) = self.locate(t);
        let segment = &self.samples[k * self.len() + body];
        if dt == 0.0 {
            return (segment.x, segment.v);
        }
        segment.state(dt, s)
    }

    /// Positions of all bodies at time `t` into `out`.
    pub fn positions(&self, t: f64, out: &mut [DVec3]) {
        let (k, dt, s) = self.locate(t);
        let n = self.len();
        for (segment, out) in self.samples[k * n..(k + 1) * n].iter().zip(out) {
            *out = if dt == 0.0 {
                segment.x
            } else {
                segment.position(dt, s)
            };
        }
    }

    /// Total energy of the bodies at the last sample (J), without the work of pushes.
    pub fn energy(&self) -> f64 {
        majors::energy(&self.bodies, &self.radau.x, &self.radau.v)
    }

    /// Total momentum at the last sample (kg·m/s).
    pub fn momentum(&self) -> DVec3 {
        majors::momentum(&self.bodies, &self.radau.v)
    }

    /// Total angular momentum at the last sample (kg·m²/s).
    pub fn angular_momentum(&self) -> DVec3 {
        majors::angular_momentum(&self.bodies, &self.radau.x, &self.radau.v)
    }
}
