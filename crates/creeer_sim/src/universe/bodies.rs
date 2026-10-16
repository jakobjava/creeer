//! Every celestial body: the majors in the ephemeris and the minors on rails.

use super::pose::Pose;
use super::terrains::BodyRef;
use creeer_math::glam::{DQuat, DVec3};
use creeer_orbit::rails::Group;
use creeer_orbit::{Body, Ephemeris, G, Push};
use rayon::prelude::*;

/// A small body on rails: it feels the majors, does not pull them.
#[derive(Clone, Debug)]
pub struct Minor {
    /// Its properties.
    pub body: Body,
    /// Its trajectory.
    pub group: Group,
}

/// The majors and the minors.
#[derive(Clone, Debug)]
pub struct Bodies {
    /// Majors: integrated together.
    pub ephemeris: Ephemeris,
    /// Minors: each on its own rails.
    pub minors: Vec<Minor>,
}

impl Bodies {
    /// A body's properties.
    pub fn body(&self, which: BodyRef) -> &Body {
        match which {
            BodyRef::Major(i) => &self.ephemeris.bodies()[i],
            BodyRef::Minor(i) => &self.minors[i].body,
        }
    }

    /// Every body with its reference.
    pub fn all(&self) -> impl Iterator<Item = (BodyRef, &Body)> {
        let majors = self
            .ephemeris
            .bodies()
            .iter()
            .enumerate()
            .map(|(i, b)| (BodyRef::Major(i), b));
        let minors = self
            .minors
            .iter()
            .enumerate()
            .map(|(i, m)| (BodyRef::Minor(i), &m.body));
        majors.chain(minors)
    }

    /// Inertial position and velocity at time `t`. A minor is known at the time its rails were
    /// last propagated to; within a tick of that it is extrapolated ([`Bodies::extrapolate`]).
    pub fn state(&self, which: BodyRef, t: f64) -> (DVec3, DVec3) {
        match which {
            BodyRef::Major(i) => self.ephemeris.state(i, t),
            BodyRef::Minor(i) => self.extrapolate(&self.minors[i].group, t),
        }
    }

    /// Body-fixed → inertial rotation at time `t`.
    pub fn orientation(&self, which: BodyRef, t: f64) -> DQuat {
        self.body(which).spin.orientation_at(t)
    }

    /// The body-fixed frame (centre at the origin, rotation axis +Y) at time `t`.
    pub fn pose(&self, which: BodyRef, t: f64) -> Pose {
        let (position, velocity) = self.state(which, t);
        let body = self.body(which);
        Pose {
            position,
            velocity,
            rotation: body.spin.orientation_at(t),
            spin: body.spin.angular_velocity(),
        }
    }

    /// Acceleration of a body by the pushes acting on it at time `t` (inertial, m/s²).
    pub fn push_acceleration(&self, which: BodyRef, t: f64) -> DVec3 {
        let (pushes, index, mass): (&[Push], usize, f64) = match which {
            BodyRef::Major(i) => (
                self.ephemeris.pushes(),
                i,
                self.ephemeris.bodies()[i].mu / G,
            ),
            BodyRef::Minor(i) => (
                &self.minors[i].group.pushes,
                0,
                self.minors[i].group.particles[0].mass,
            ),
        };
        pushes
            .iter()
            .filter(|p| p.body == index && p.active(t, false))
            .fold(DVec3::ZERO, |a, p| a + p.force / mass)
    }

    /// Adds a push on a body (inertial force over [start, end)). A push on a major drops the
    /// forecast after its start; it is computed again right away (to `start` + `ahead`), so
    /// the present stays covered.
    pub fn push(&mut self, which: BodyRef, start: f64, end: f64, force: DVec3, ahead: f64) {
        match which {
            BodyRef::Major(i) => {
                self.ephemeris.push(Push {
                    body: i,
                    start,
                    end,
                    force,
                });
                self.ephemeris.extend_to(end.max(start + ahead));
            }
            BodyRef::Minor(i) => self.minors[i].group.push(Push {
                body: 0,
                start,
                end,
                force,
            }),
        }
    }

    /// Propagates every minor except `skip` to time `t` (in parallel). Minors that hit a body
    /// stop there; their indices are returned.
    pub fn propagate_minors(&mut self, t: f64, skip: Option<usize>) -> Vec<(usize, usize)> {
        let eph = &self.ephemeris;
        self.minors
            .par_iter_mut()
            .enumerate()
            .filter(|(i, m)| Some(*i) != skip && m.group.time() < t)
            .filter_map(|(i, m)| {
                m.group
                    .propagate(eph, t, Some(0.0))
                    .map(|hit| (i, hit.body))
            })
            .collect()
    }
}

impl Bodies {
    /// State of a rails particle (the first of `group`) at a time near its own: a Taylor step
    /// of second order in the majors' gravity and its pushes. Used within a physics step (half
    /// a tick: 8 ms), where it is exact to 10⁻⁹ m; the rails themselves are never advanced by
    /// it.
    ///
    /// # Panics
    /// In debug builds, if `t` is more than a minute from the group's time.
    pub fn extrapolate(&self, group: &Group, t: f64) -> (DVec3, DVec3) {
        let (x, v) = group.state(0);
        let dt = t - group.time();
        if dt == 0.0 {
            return (x, v);
        }
        debug_assert!(dt.abs() < 60.0, "extrapolating rails by {dt} s");
        let t0 = group.time();
        let mut a = DVec3::ZERO;
        for (i, body) in self.ephemeris.bodies().iter().enumerate() {
            let d = self.ephemeris.position(i, t0) - x;
            let r = d.length();
            a += d * (body.mu / (r * r * r));
        }
        let particle = group.particles[0];
        for push in &group.pushes {
            if push.body == 0 && push.active(t0, false) {
                a += push.force / particle.mass;
            }
        }
        (x + v * dt + a * (0.5 * dt * dt), v + a * dt)
    }
}
