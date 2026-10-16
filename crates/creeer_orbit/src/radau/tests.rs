use super::*;
use crate::kepler;
use std::f64::consts::TAU;

/// Point masses attracting each other: μᵢ = G·mᵢ.
struct PointMasses(Vec<f64>);

impl Forces for PointMasses {
    fn accelerations(&mut self, _t: f64, x: &[DVec3], _v: &[DVec3], a: &mut [DVec3]) {
        a.fill(DVec3::ZERO);
        for i in 0..x.len() {
            for j in i + 1..x.len() {
                let d = x[j] - x[i];
                let r2 = d.length_squared();
                let inv_r3 = 1.0 / (r2 * r2.sqrt());
                a[i] += d * (self.0[j] * inv_r3);
                a[j] -= d * (self.0[i] * inv_r3);
            }
        }
    }
}

/// A prescribed acceleration a(t) along x.
struct Prescribed(fn(f64) -> f64);

impl Forces for Prescribed {
    fn accelerations(&mut self, t: f64, _x: &[DVec3], _v: &[DVec3], a: &mut [DVec3]) {
        a[0] = DVec3::X * (self.0)(t);
    }
}

fn legendre(n: usize, x: f64) -> f64 {
    let (mut p0, mut p1) = (1.0, x);
    for k in 1..n {
        #[allow(clippy::cast_precision_loss)]
        let k = k as f64;
        let p2 = ((2.0 * k + 1.0) * x * p1 - k * p0) / (k + 1.0);
        p0 = p1;
        p1 = p2;
    }
    p1
}

#[test]
fn spacings_are_the_gauss_radau_nodes() {
    for &h in &H[1..] {
        let x = 2.0 * h - 1.0;
        let residual = legendre(7, x) + legendre(8, x);
        assert!(residual.abs() < 1e-14, "h = {h}: {residual}");
    }
}

#[test]
fn a_degree_seven_acceleration_is_integrated_exactly_in_one_step() {
    // a = t⁷ → v = t⁸/8, x = t⁹/72.
    let mut forces = Prescribed(|t| t * t * t * t * t * t * t);
    let mut radau = Radau::new(0.0, vec![DVec3::ZERO], vec![DVec3::ZERO], 2.0);
    // Error control would refine near t = 0, where a vanishes: force the single step.
    radau.min_dt = 2.0;
    let taken = radau.step(&mut forces, 2.0);
    assert_eq!(taken, 2.0);
    assert!(
        (radau.v[0].x - 256.0 / 8.0).abs() < 1e-13,
        "{}",
        radau.v[0].x
    );
    assert!(
        (radau.x[0].x - 512.0 / 72.0).abs() < 1e-13,
        "{}",
        radau.x[0].x
    );
}

/// Integrates two bodies for `orbits` periods; returns the relative state, the step count,
/// and the Kepler solution.
fn two_body(e: f64, orbits: f64) -> ((DVec3, DVec3), u64, (DVec3, DVec3), f64) {
    let (m1, m2) = (3.986e14, 4.9e12); // Earth and Moon μ
    let mu = m1 + m2;
    let elements = kepler::Elements::from_semi_major_axis(mu, 3.84e8, e, [0.2, 0.3, 0.4, 0.0]);
    let (r, w) = elements.to_state();
    // Barycentric start.
    let x = vec![-r * (m2 / mu), r * (m1 / mu)];
    let v = vec![-w * (m2 / mu), w * (m1 / mu)];
    let mut forces = PointMasses(vec![m1, m2]);
    let mut radau = Radau::new(0.0, x, v, 100.0);
    let period = elements.period();
    let mut steps = 0;
    radau.integrate_to(&mut forces, orbits * period, |_| steps += 1);
    let rel = (radau.x[1] - radau.x[0], radau.v[1] - radau.v[0]);
    let expected = kepler::propagate(mu, r, w, orbits * period);
    let energy = |(r, v): (DVec3, DVec3)| 0.5 * v.length_squared() - mu / r.length();
    let drift = ((energy(rel) - energy((r, w))) / energy((r, w))).abs();
    (rel, steps, expected, drift)
}

#[test]
fn a_thousand_orbits_follow_kepler_to_rounding() {
    for (e, max_phase_error) in [(0.0, 1e-9), (0.5, 1e-9), (0.9, 1e-8), (0.99, 1e-7)] {
        let (rel, steps, expected, drift) = two_body(e, 1000.0);
        let a = 3.84e8;
        let error = (rel.0 - expected.0).length() / a;
        println!(
            "e = {e}: {} steps per orbit, position error {error:.2e} of a, energy drift {drift:.2e}",
            steps / 1000
        );
        assert!(error < max_phase_error, "e = {e}: error {error:e}");
        // No secular energy drift: only accumulated rounding.
        assert!(drift < 1e-12, "e = {e}: energy drift {drift:e}");
        let h0 = expected.0.cross(expected.1).length();
        let h = rel.0.cross(rel.1).length();
        assert!((h - h0).abs() / h0 < 1e-12, "e = {e}: angular momentum");
    }
}

#[test]
fn a_near_parabolic_close_encounter_is_resolved() {
    // A hyperbolic flyby grazing 1.01 radii of a moon-sized body.
    let mu = 4.9e12;
    let rp = 1.76e6;
    let elements = kepler::Elements {
        mu,
        p: rp * (1.0 + 1.0001),
        e: 1.0001,
        inclination: 0.0,
        node: 0.0,
        periapsis_argument: 0.0,
        true_anomaly: -2.5,
    };
    let (r, v) = elements.to_state();
    let mut forces = PointMasses(vec![mu, 0.0]);
    let mut radau = Radau::new(0.0, vec![DVec3::ZERO, r], vec![DVec3::ZERO, v], 60.0);
    let t_end = 40_000.0;
    let mut closest = f64::INFINITY;
    radau.integrate_to(&mut forces, t_end, |s| {
        closest = closest.min((s.x[1] - s.x[0]).length());
    });
    let expected = kepler::propagate(mu, r, v, t_end);
    let error = (radau.x[1] - expected.0).length();
    assert!(error < 1e-3, "error {error} m after the flyby");
    assert!(closest < rp * 1.5);
}

/// Chenciner & Montgomery's figure-eight choreography (G = m = 1, period 6.32591398).
#[test]
fn the_figure_eight_returns_after_one_period() {
    let x1 = DVec3::new(0.970_004_36, -0.243_087_53, 0.0);
    let v3 = DVec3::new(-0.932_407_37, -0.864_731_46, 0.0);
    let x = vec![x1, -x1, DVec3::ZERO];
    let v = vec![-v3 * 0.5, -v3 * 0.5, v3];
    let mut forces = PointMasses(vec![1.0; 3]);
    let mut radau = Radau::new(0.0, x.clone(), v.clone(), 0.01);
    radau.integrate_to(&mut forces, 6.325_913_98, |_| {});
    for i in 0..3 {
        // Limited by the 8 digits of the published initial conditions.
        assert!((radau.x[i] - x[i]).length() < 1e-6, "body {i}");
        assert!((radau.v[i] - v[i]).length() < 1e-6, "body {i}");
    }
}

#[test]
fn compensated_summation_keeps_increments_below_the_last_bit() {
    // 1.6e-13 m/s² on a body moving at 30 km/s: each 60 s step adds 1e-11 m/s, about 3 ulps
    // of the velocity. Plain summation would round a large part of it away every step.
    let a = 1.6e-13;
    let mut forces = Prescribed(|_| 1.6e-13);
    let v0 = 3.0e4;
    let mut radau = Radau::new(0.0, vec![DVec3::ZERO], vec![DVec3::X * v0], 60.0);
    let steps = 100_000;
    for _ in 0..steps {
        let t = radau.t;
        radau.step(&mut forces, t + 60.0);
    }
    let t = 60.0 * f64::from(steps);
    let gained = radau.v[0].x - v0;
    assert!(
        (gained - a * t).abs() < 1e-3 * a * t,
        "gained {gained:e} vs {:e}",
        a * t
    );
    let travelled = radau.x[0].x - v0 * t;
    assert!((travelled - 0.5 * a * t * t).abs() < 1e-3 * 0.5 * a * t * t);
}

#[test]
fn steps_cut_to_exact_times_agree_with_free_steps() {
    let (_, _, _, _) = two_body(0.3, 1.0);
    let mu = 3.986e14;
    let (r, v) =
        kepler::Elements::from_semi_major_axis(mu, 1e7, 0.3, [0.1, 0.2, 0.3, 0.0]).to_state();
    let mut forces = PointMasses(vec![mu, 0.0]);
    let start = || Radau::new(0.0, vec![DVec3::ZERO, r], vec![DVec3::ZERO, v], 10.0);
    let mut free = start();
    free.integrate_to(&mut forces, 30_000.0, |_| {});
    // The same span in 1/60 s game ticks for a while, then in 1000 s chunks.
    let mut ticked = start();
    for k in 1..=600 {
        ticked.integrate_to(&mut forces, f64::from(k) / 60.0, |_| {});
    }
    for k in 1..=30 {
        ticked.integrate_to(&mut forces, f64::from(k) * 1000.0, |_| {});
    }
    let error = (free.x[1] - ticked.x[1]).length();
    assert!(error < 1e-5, "error {error} m");
    let expected = kepler::propagate(mu, r, v, 30_000.0).0;
    assert!((ticked.x[1] - expected).length() < 1e-5);
}

#[test]
fn steps_are_a_fraction_of_the_orbital_period() {
    let (_, steps, _, _) = two_body(0.0, 10.0);
    // (ε·7!)^{1/7}·T/2π = T/36 for ε = 1e-9.
    let per_orbit = steps / 10;
    assert!(
        (30..=42).contains(&per_orbit),
        "{per_orbit} steps per orbit"
    );
    let _ = TAU;
}
