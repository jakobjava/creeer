//! Numerical stability checks for a generated system.
//!
//! A random system can look fine and fall apart after a few centuries: a moon ejected, two
//! planets swapping orbits. The analytic spacing rules make that unlikely; these checks
//! make sure. The whole system at once would be slow (its steps are set by the fastest
//! moon), so it is checked in parts, each with the bodies that shape it:
//! - the planets (each carrying its moons' mass) about the star, for many orbits of the
//!   innermost planet;
//! - every moon system (star, planet, moons) for many orbits of its outermost moon.

use super::Generated;
use super::def::Stability;
use crate::body::Body;
use crate::kepler::Elements;
use crate::majors::MajorForces;
use crate::radau::Radau;
use creeer_math::glam::DVec3;
use rayon::prelude::*;

/// Integrates `bodies` and calls `check` after every step; stops at the first failure.
fn run(
    bodies: &[Body],
    x: Vec<DVec3>,
    v: Vec<DVec3>,
    t_end: f64,
    mut check: impl FnMut(&Radau) -> Result<(), String>,
) -> Result<(), String> {
    let mut radau = Radau::new(0.0, x, v, 1.0);
    let mut forces = MajorForces {
        bodies,
        pushes: &[],
        left: false,
    };
    while radau.t < t_end {
        radau.step(&mut forces, t_end);
        check(&radau)?;
    }
    Ok(())
}

/// Semi-major axis and eccentricity of body `i` about body `c`.
fn orbit(bodies: &[Body], r: &Radau, i: usize, c: usize) -> (f64, f64) {
    let el = Elements::from_state(
        bodies[i].mu + bodies[c].mu,
        r.x[i] - r.x[c],
        r.v[i] - r.v[c],
    );
    (el.semi_major_axis(), el.e)
}

/// Checks the planets about the star.
pub fn planets(g: &Generated, s: &Stability) -> Result<(), String> {
    let planets: Vec<usize> = (1..g.majors.len())
        .filter(|&i| g.parents[i] == Some(0))
        .collect();
    let mut bodies = vec![g.majors[0].body.clone()];
    let (mut x, mut v) = (vec![g.majors[0].x], vec![g.majors[0].v]);
    for &p in &planets {
        // The planet with its moons as one body at their barycentre.
        let mut body = g.majors[p].body.clone();
        let (mut mx, mut mv) = (g.majors[p].x * body.mu, g.majors[p].v * body.mu);
        for (m, placed) in g.majors.iter().enumerate() {
            if g.parents[m] == Some(p) {
                body.mu += placed.body.mu;
                mx += placed.x * placed.body.mu;
                mv += placed.v * placed.body.mu;
            }
        }
        x.push(mx / body.mu);
        v.push(mv / body.mu);
        bodies.push(body);
    }
    let start: Vec<f64> = (1..bodies.len())
        .map(|i| {
            let el = Elements::from_state(bodies[0].mu + bodies[i].mu, x[i] - x[0], v[i] - v[0]);
            el.semi_major_axis()
        })
        .collect();
    let shortest = start
        .iter()
        .zip(&bodies[1..])
        .map(|(a, b)| std::f64::consts::TAU * (a * a * a / (bodies[0].mu + b.mu)).sqrt())
        .fold(f64::INFINITY, f64::min);
    let t_end = s.planet_orbits * shortest;
    run(&bodies, x, v, t_end, |r| {
        for i in 1..bodies.len() {
            let (a, e) = orbit(&bodies, r, i, 0);
            if !(a > 0.0 && e < 0.6 && (a / start[i - 1] - 1.0).abs() < 0.02) {
                return Err(format!(
                    "{} left its orbit after {:.1} years (a {:.3e} → {a:.3e} m, e {e:.2})",
                    bodies[i].name,
                    r.t / 3.156e7,
                    start[i - 1]
                ));
            }
        }
        Ok(())
    })
}

/// Checks every planet's moons (with the star's tides).
pub fn moons(g: &Generated, s: &Stability) -> Result<(), String> {
    // The moon systems are independent: check them in parallel, report the first failure
    // in body order (the same whatever the threads do).
    let results: Vec<Result<(), String>> = (1..g.majors.len())
        .into_par_iter()
        .map(|p| moon_system(g, s, p))
        .collect();
    results.into_iter().collect()
}

fn moon_system(g: &Generated, s: &Stability, p: usize) -> Result<(), String> {
    {
        let moons: Vec<usize> = (0..g.majors.len())
            .filter(|&m| g.parents[m] == Some(p))
            .collect();
        if moons.is_empty() {
            return Ok(());
        }
        let members: Vec<usize> = [0, p].into_iter().chain(moons.iter().copied()).collect();
        let bodies: Vec<Body> = members.iter().map(|&i| g.majors[i].body.clone()).collect();
        let x: Vec<DVec3> = members.iter().map(|&i| g.majors[i].x).collect();
        let v: Vec<DVec3> = members.iter().map(|&i| g.majors[i].v).collect();
        let start: Vec<f64> = (2..bodies.len())
            .map(|i| {
                Elements::from_state(bodies[1].mu + bodies[i].mu, x[i] - x[1], v[i] - v[1])
                    .semi_major_axis()
            })
            .collect();
        let outer = start.iter().copied().fold(0.0, f64::max);
        let period = std::f64::consts::TAU * (outer * outer * outer / bodies[1].mu).sqrt();
        run(&bodies, x, v, s.moon_orbits * period, |r| {
            for i in 2..bodies.len() {
                let (a, e) = orbit(&bodies, r, i, 1);
                let periapsis = a * (1.0 - e);
                let ok = a > 0.0
                    && e < 0.5
                    && periapsis > 1.5 * bodies[1].radius
                    && (a / start[i - 2] - 1.0).abs() < 0.05;
                if !ok {
                    return Err(format!(
                        "moon {} of {} became unstable after {:.0} days (a {a:.3e} m, e {e:.2})",
                        bodies[i].name,
                        bodies[1].name,
                        r.t / 86_400.0
                    ));
                }
            }
            Ok(())
        })
    }
}
