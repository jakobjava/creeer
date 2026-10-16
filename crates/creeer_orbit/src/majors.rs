//! Forces between the major bodies: mutual gravity plus pushes.
//!
//! A **push** is a force on a body over an interval of time: an engine, a mass driver, the
//! reaction of a ship pulling on it, an impact spread over a tick. Pushes switch on and off
//! discontinuously, so integration steps end exactly where one starts or stops.

use crate::body::{Body, G};
use crate::radau::Forces;
use creeer_math::glam::DVec3;

/// A constant force on a body over a time interval.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Push {
    /// Index of the body.
    pub body: usize,
    /// Start time (s): the force acts from here on ...
    pub start: f64,
    /// ... until here (s), exclusive.
    pub end: f64,
    /// Force in inertial coordinates (N).
    pub force: DVec3,
}

impl Push {
    /// Whether the push acts at time `t`. At its edges, `left` asks for the limit from below
    /// (the end of the step before the edge), otherwise from above.
    pub fn active(&self, t: f64, left: bool) -> bool {
        if left {
            self.start < t && t <= self.end
        } else {
            self.start <= t && t < self.end
        }
    }

    /// Impulse delivered (N·s).
    pub fn impulse(&self) -> DVec3 {
        self.force * (self.end - self.start)
    }
}

/// Gravity between all bodies (each a point mass) plus the pushes on them.
#[derive(Debug)]
pub struct MajorForces<'a> {
    /// The bodies (their μ).
    pub bodies: &'a [Body],
    /// Pushes, any order.
    pub pushes: &'a [Push],
    /// Evaluate pushes as limits from below at their edges.
    pub left: bool,
}

impl Forces for MajorForces<'_> {
    fn accelerations(&mut self, t: f64, x: &[DVec3], _v: &[DVec3], a: &mut [DVec3]) {
        gravity(self.bodies, x, a);
        for push in self.pushes {
            if push.active(t, self.left) {
                // a = F/M with M = μ/G.
                a[push.body] += push.force * (G / self.bodies[push.body].mu);
            }
        }
    }
}

/// Mutual point-mass gravity, pair by pair in a fixed order (deterministic).
pub fn gravity(bodies: &[Body], x: &[DVec3], a: &mut [DVec3]) {
    a.fill(DVec3::ZERO);
    for i in 0..x.len() {
        for j in i + 1..x.len() {
            let d = x[j] - x[i];
            let r2 = d.length_squared();
            let inv_r3 = 1.0 / (r2 * r2.sqrt());
            a[i] += d * (bodies[j].mu * inv_r3);
            a[j] -= d * (bodies[i].mu * inv_r3);
        }
    }
}

/// The first push edge (start or end) after `t`, if any.
pub fn next_edge(pushes: &[Push], t: f64) -> Option<f64> {
    pushes
        .iter()
        .flat_map(|p| [p.start, p.end])
        .filter(|&edge| edge > t)
        .min_by(f64::total_cmp)
}

/// Total energy of the bodies (J): kinetic plus mutual potential.
pub fn energy(bodies: &[Body], x: &[DVec3], v: &[DVec3]) -> f64 {
    let mut kinetic = 0.0;
    let mut potential = 0.0;
    for i in 0..x.len() {
        kinetic += 0.5 * bodies[i].mass() * v[i].length_squared();
        for j in i + 1..x.len() {
            potential -= bodies[i].mass() * bodies[j].mu / (x[j] - x[i]).length();
        }
    }
    kinetic + potential
}

/// Total momentum (kg·m/s).
pub fn momentum(bodies: &[Body], v: &[DVec3]) -> DVec3 {
    bodies
        .iter()
        .zip(v)
        .fold(DVec3::ZERO, |p, (b, v)| p + *v * b.mass())
}

/// Total angular momentum about the origin (kg·m²/s).
pub fn angular_momentum(bodies: &[Body], x: &[DVec3], v: &[DVec3]) -> DVec3 {
    bodies
        .iter()
        .zip(x.iter().zip(v))
        .fold(DVec3::ZERO, |l, (b, (x, v))| l + x.cross(*v) * b.mass())
}
