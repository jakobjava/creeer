//! Particles on rails: ships and small bodies that feel every major body but do not pull on
//! them (the restricted n-body problem, ADR-0007).
//!
//! A [`Group`] is a set of particles integrated together: usually one ship alone, but a ship
//! next to an asteroid it is towing by gravity forms a group, so each pulls the other
//! (Newton's third law holds inside a group). The majors come from the [`Ephemeris`],
//! which must cover the span being propagated. Groups are independent of each other and can
//! be propagated in parallel.
//!
//! Propagation stops early when a particle comes closer to a body than a given altitude
//! above its surface ([`Approach`]): the simulation then loads it into a physics bubble, or
//! it has hit the body.

use crate::ephemeris::Ephemeris;
use crate::majors::{self, Push};
use crate::radau::{Forces, Radau, Segment};
use creeer_math::glam::DVec3;

/// A particle's own physical properties.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Particle {
    /// Mass (kg): pushes accelerate it by F/m.
    pub mass: f64,
    /// Its own gravitational parameter (m³/s²), felt by the others in its group only.
    pub mu: f64,
}

/// A particle came within `altitude` of a body's surface.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Approach {
    /// Index of the particle in its group.
    pub particle: usize,
    /// Index of the body.
    pub body: usize,
    /// Time of the crossing (s).
    pub t: f64,
}

/// Particles integrated together against the majors.
#[derive(Clone, Debug)]
pub struct Group {
    /// The integrator (positions and velocities are inertial).
    pub radau: Radau,
    /// The particles.
    pub particles: Vec<Particle>,
    /// Pushes on them (`Push::body` is the particle's index).
    pub pushes: Vec<Push>,
}

struct RailsForces<'a> {
    eph: &'a Ephemeris,
    particles: &'a [Particle],
    pushes: &'a [Push],
    majors: Vec<DVec3>,
}

impl Forces for RailsForces<'_> {
    fn accelerations(&mut self, t: f64, x: &[DVec3], _v: &[DVec3], a: &mut [DVec3]) {
        self.eph.positions(t, &mut self.majors);
        let bodies = self.eph.bodies();
        for (x, a) in x.iter().zip(a.iter_mut()) {
            let mut sum = DVec3::ZERO;
            for (body, p) in bodies.iter().zip(&self.majors) {
                let d = *p - *x;
                let r2 = d.length_squared();
                sum += d * (body.mu / (r2 * r2.sqrt()));
            }
            *a = sum;
        }
        // Mutual gravity inside the group, pair by pair.
        for i in 0..x.len() {
            for j in i + 1..x.len() {
                let (mi, mj) = (self.particles[i].mu, self.particles[j].mu);
                if mi == 0.0 && mj == 0.0 {
                    continue;
                }
                let d = x[j] - x[i];
                let r2 = d.length_squared();
                let inv_r3 = 1.0 / (r2 * r2.sqrt());
                a[i] += d * (mj * inv_r3);
                a[j] -= d * (mi * inv_r3);
            }
        }
        for push in self.pushes {
            if push.active(t, false) {
                a[push.body] += push.force / self.particles[push.body].mass;
            }
        }
    }
}

impl Group {
    /// Particles with inertial states `(x, v)` at time `t`.
    pub fn new(t: f64, states: &[(DVec3, DVec3)], particles: Vec<Particle>) -> Self {
        assert_eq!(states.len(), particles.len(), "one state per particle");
        let x = states.iter().map(|s| s.0).collect();
        let v = states.iter().map(|s| s.1).collect();
        Self {
            // The first step adapts itself; a minute is a harmless start.
            radau: Radau::new(t, x, v, 60.0),
            particles,
            pushes: Vec::new(),
        }
    }

    /// Current time (s).
    pub fn time(&self) -> f64 {
        self.radau.t
    }

    /// Inertial position and velocity of particle `i`.
    pub fn state(&self, i: usize) -> (DVec3, DVec3) {
        (self.radau.x[i], self.radau.v[i])
    }

    /// Adds a push on one of the particles (it must not start in the past).
    pub fn push(&mut self, push: Push) {
        assert!(push.start >= self.radau.t, "the past cannot be pushed");
        self.pushes.push(push);
    }

    /// Propagates until `until`, or until a particle comes within `altitude` of a body's
    /// surface (approaching from farther out; `None`: never stop). The ephemeris must cover
    /// the span.
    pub fn propagate(
        &mut self,
        eph: &Ephemeris,
        until: f64,
        altitude: Option<f64>,
    ) -> Option<Approach> {
        let n = self.radau.len();
        let mut forces = RailsForces {
            eph,
            particles: &self.particles,
            pushes: &self.pushes,
            majors: vec![DVec3::ZERO; eph.len()],
        };
        let mut start = Vec::with_capacity(n);
        while self.radau.t < until {
            let limit =
                majors::next_edge(&self.pushes, self.radau.t).map_or(until, |edge| edge.min(until));
            let t0 = self.radau.t;
            start.clear();
            {
                let a = self.radau.accelerations(&mut forces).to_vec();
                for (i, a) in a.into_iter().enumerate() {
                    let (cx, cv) = self.radau.compensation(i);
                    start.push(Segment {
                        x: self.radau.x[i],
                        v: self.radau.v[i],
                        comp: [cx, cv],
                        a,
                        b: [DVec3::ZERO; 7],
                    });
                }
            }
            let dt = self.radau.step(&mut forces, limit);
            for (i, segment) in start.iter_mut().enumerate() {
                segment.b = *self.radau.last_fit(i);
            }
            let hit = altitude.and_then(|altitude| first_approach(eph, &start, t0, dt, altitude));
            if let Some(hit) = hit {
                // Stop at the crossing.
                let s = (hit.t - t0) / dt;
                let states: Vec<[DVec3; 4]> = start
                    .iter()
                    .map(|seg| {
                        let (x, v) = seg.state(dt, s);
                        [x, v, DVec3::ZERO, DVec3::ZERO]
                    })
                    .collect();
                self.radau.restore(hit.t, states);
                return Some(hit);
            }
        }
        None
    }
}

/// The first time inside a step at which a particle that started outside a body's threshold
/// sphere (radius + `altitude`) is inside it, found by bisection on the dense output.
#[allow(clippy::needless_range_loop)] // indices feed the closure
fn first_approach(
    eph: &Ephemeris,
    start: &[Segment],
    t0: f64,
    dt: f64,
    altitude: f64,
) -> Option<Approach> {
    let bodies = eph.bodies();
    let inside = |particle: usize, body: usize, s: f64| {
        let t = t0 + s * dt;
        let d = (start[particle].position(dt, s) - eph.position(body, t)).length();
        d < bodies[body].radius + altitude
    };
    let mut best: Option<Approach> = None;
    for particle in 0..start.len() {
        for body in 0..bodies.len() {
            // Cheap reject: farther than the step could bring it.
            let gap = (start[particle].x - eph.position(body, t0)).length()
                - bodies[body].radius
                - altitude;
            let reach =
                (start[particle].v.length() + start[particle].a.length() * dt) * dt * 2.0 + 1e5;
            if gap > reach || inside(particle, body, 0.0) {
                continue;
            }
            // Coarse scan for the first inside point, then bisection.
            const SCAN: u32 = 16;
            let Some(k) = (1..=SCAN).find(|&k| inside(particle, body, f64::from(k) / 16.0)) else {
                continue;
            };
            let (mut lo, mut hi) = (f64::from(k - 1) / 16.0, f64::from(k) / 16.0);
            for _ in 0..60 {
                let mid = 0.5 * (lo + hi);
                if inside(particle, body, mid) {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
            let t = t0 + hi * dt;
            if best.is_none_or(|b| t < b.t) {
                best = Some(Approach { particle, body, t });
            }
        }
    }
    best
}
