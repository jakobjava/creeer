//! F0 tech sandbox simulation: test masses orbiting a point mass.
//!
//! This is a stand-in for the real simulation, used to exercise the simulation thread,
//! interpolation and camera-relative rendering. It is still integrated properly: velocity
//! Verlet (leapfrog) is symplectic, so energy oscillates slightly but does not drift, the
//! same property the orbital-mechanics code in F4 needs.

use creeer_core::Simulation;
use creeer_math::glam::{DQuat, DVec3};

/// Gravitational parameter μ = G·M of the central mass (m³/s²). Chosen so a 10 m orbit
/// takes about 9 s.
pub const MU: f64 = 500.0;

#[derive(Clone, Copy, Debug)]
struct Body {
    position: DVec3,
    velocity: DVec3,
}

/// Several bodies orbiting a fixed point mass at the origin.
#[derive(Debug)]
pub struct OrbitSandbox {
    bodies: Vec<Body>,
    time: f64,
    initial_energy: f64,
}

/// State handed to the renderer after each tick.
#[derive(Clone, Debug)]
pub struct SandboxSnapshot {
    /// Body positions relative to the central mass (m).
    pub positions: Vec<DVec3>,
    /// Simulated time (s).
    pub time: f64,
    /// Relative change of total energy since the start, (E − E₀) / |E₀|.
    pub energy_drift: f64,
}

impl OrbitSandbox {
    /// A demo system: a dozen orbits of different radius, eccentricity and inclination.
    pub fn demo() -> Self {
        let bodies = (0..12u32)
            .map(|i| {
                let i = f64::from(i);
                let radius = 6.0 + 2.0 * i;
                // 0.8..1.15 of circular speed gives a mix of elliptical orbits.
                let speed = circular_speed(radius) * (0.8 + 0.03 * i);
                let tilt = DQuat::from_rotation_x(0.08 * (i - 5.0));
                let start = DQuat::from_rotation_y(0.9 * i);
                let orient = start * tilt;
                (
                    orient * DVec3::new(radius, 0.0, 0.0),
                    orient * DVec3::new(0.0, 0.0, -speed),
                )
            })
            .collect();
        Self::with_bodies(bodies)
    }

    /// Bodies given as `(position, velocity)` relative to the central mass.
    pub fn with_bodies(bodies: Vec<(DVec3, DVec3)>) -> Self {
        let bodies: Vec<Body> = bodies
            .into_iter()
            .map(|(position, velocity)| Body { position, velocity })
            .collect();
        let initial_energy = total_energy(&bodies);
        Self {
            bodies,
            time: 0.0,
            initial_energy,
        }
    }
}

impl Simulation for OrbitSandbox {
    type Snapshot = SandboxSnapshot;

    fn step(&mut self, dt: f64) {
        // Velocity Verlet: half kick, drift, half kick.
        for body in &mut self.bodies {
            body.velocity += acceleration(body.position) * (0.5 * dt);
            body.position += body.velocity * dt;
            body.velocity += acceleration(body.position) * (0.5 * dt);
        }
        self.time += dt;
    }

    fn snapshot(&self) -> SandboxSnapshot {
        SandboxSnapshot {
            positions: self.bodies.iter().map(|b| b.position).collect(),
            time: self.time,
            energy_drift: (total_energy(&self.bodies) - self.initial_energy)
                / self.initial_energy.abs(),
        }
    }
}

/// Speed of a circular orbit of radius `r`: v = √(μ / r).
pub fn circular_speed(r: f64) -> f64 {
    (MU / r).sqrt()
}

/// Newtonian gravity of the central mass: a = −μ r̂ / r².
fn acceleration(position: DVec3) -> DVec3 {
    let r2 = position.length_squared();
    -position * (MU / (r2 * r2.sqrt()))
}

/// Specific orbital energy summed over bodies: ½v² − μ/r.
fn total_energy(bodies: &[Body]) -> f64 {
    bodies
        .iter()
        .map(|b| 0.5 * b.velocity.length_squared() - MU / b.position.length())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::TAU;

    const DT: f64 = 1.0 / 60.0;

    fn run(sim: &mut OrbitSandbox, seconds: f64) {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let steps = (seconds / DT).round() as u64;
        for _ in 0..steps {
            sim.step(DT);
        }
    }

    /// Kepler's third law: a circular orbit returns to its start after T = 2π √(r³/μ).
    #[test]
    fn circular_orbit_has_keplerian_period() {
        let r = 10.0;
        let start = DVec3::new(r, 0.0, 0.0);
        let mut sim =
            OrbitSandbox::with_bodies(vec![(start, DVec3::new(0.0, 0.0, -circular_speed(r)))]);
        let period = TAU * (r * r * r / MU).sqrt();
        // Integrate a whole number of steps closest to one period.
        run(&mut sim, period);
        #[allow(clippy::cast_precision_loss)]
        let simulated = (period / DT).round() * DT;
        let expected = DQuat::from_rotation_y((simulated - period) / period * TAU) * start;
        let end = sim.snapshot().positions[0];
        let error = (end - expected).length();
        assert!(error < 0.01, "after one period the body is {error} m off");
        assert!(
            (end.length() - r).abs() < 1e-3,
            "radius drifted to {}",
            end.length()
        );
    }

    /// The integrator must be symplectic: over 100 orbits energy may wobble, never drift.
    #[test]
    fn energy_is_conserved_over_many_orbits() {
        let mut sim = OrbitSandbox::demo();
        let longest_period = TAU * (28.0f64 * 28.0 * 28.0 / MU).sqrt();
        let mut worst: f64 = 0.0;
        for _ in 0..100 {
            run(&mut sim, longest_period);
            worst = worst.max(sim.snapshot().energy_drift.abs());
        }
        assert!(worst < 1e-3, "energy drifted by {worst:e}");
    }

    #[test]
    fn demo_orbits_are_bound() {
        let sim = OrbitSandbox::demo();
        for b in &sim.bodies {
            let energy = 0.5 * b.velocity.length_squared() - MU / b.position.length();
            assert!(energy < 0.0, "a body would escape");
        }
    }
}
