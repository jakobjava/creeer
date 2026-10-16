use super::*;
use crate::body::{BodyKind, G, Spin};
use crate::kepler;

fn body(name: &str, mu: f64, radius: f64) -> Body {
    Body {
        name: name.into(),
        kind: BodyKind::Planet,
        mu,
        radius,
        spin: Spin::new(f64::INFINITY, 0.0, 0.0),
        color: [255; 3],
        seed: 0,
        surface: None,
    }
}

const EARTH: f64 = 3.986e14;
const MOON: f64 = 4.9e12;

/// Earth and Moon (e = 0.2) about their barycentre; returns the ephemeris and the relative
/// start state.
fn earth_moon() -> (Ephemeris, DVec3, DVec3) {
    let mu = EARTH + MOON;
    let (r, w) =
        kepler::Elements::from_semi_major_axis(mu, 3.84e8, 0.2, [0.3, 0.2, 0.1, 0.0]).to_state();
    let x = vec![-r * (MOON / mu), r * (EARTH / mu)];
    let v = vec![-w * (MOON / mu), w * (EARTH / mu)];
    let bodies = vec![body("Earth", EARTH, 6.4e6), body("Moon", MOON, 1.7e6)];
    (Ephemeris::new(bodies, x, v, 0.0), r, w)
}

#[test]
fn interpolated_positions_follow_kepler() {
    let (mut eph, r, w) = earth_moon();
    let period = 27.3 * 86_400.0;
    eph.extend_to(3.0 * period);
    let mut worst = 0.0f64;
    let mut worst_v = 0.0f64;
    for i in 0..=997 {
        // Times off the sample grid.
        let t = 3.0 * period * f64::from(i) / 997.0;
        let rel = eph.position(1, t) - eph.position(0, t);
        let (expected, expected_v) = kepler::propagate(EARTH + MOON, r, w, t);
        worst = worst.max((rel - expected).length());
        let (_, v1) = eph.state(1, t);
        let (_, v0) = eph.state(0, t);
        worst_v = worst_v.max(((v1 - v0) - expected_v).length());
    }
    // The dense output of the integrator itself: millimetres on a 384,000 km orbit.
    assert!(worst < 5e-3, "worst position error {worst} m");
    assert!(worst_v < 1e-7, "worst velocity error {worst_v} m/s");
    println!(
        "{} samples over 3 orbits, worst error {worst:.3} m, {worst_v:.2e} m/s",
        eph.sample_count()
    );
}

/// The same push, known from the start or added after the forecast passed it, gives the
/// same trajectory.
#[test]
fn a_push_added_later_rewrites_the_forecast() {
    let period = 27.3 * 86_400.0;
    let push = Push {
        body: 1,
        start: 0.37 * period,
        end: 0.37 * period + 3600.0,
        force: DVec3::new(1e17, 2e16, -3e16),
    };
    let (mut known, _, _) = earth_moon();
    known.push(push);
    known.extend_to(2.0 * period);
    let (mut late, _, _) = earth_moon();
    late.extend_to(1.5 * period);
    let before = late.position(1, 1.2 * period);
    late.push(push);
    assert!(late.end() <= push.start, "forecast dropped");
    late.extend_to(2.0 * period);
    for i in 0..=50 {
        let t = 2.0 * period * f64::from(i) / 50.0;
        let error = (known.position(1, t) - late.position(1, t)).length();
        assert!(error < 1e-3, "at {t}: {error} m");
    }
    // And the push changed the Moon's path by about Δv·t.
    let dv = push.force.length() * G / MOON * 3600.0;
    let moved = (late.position(1, 1.2 * period) - before).length();
    let expected = dv * (1.2 - 0.37) * period;
    assert!(
        moved > 0.2 * expected && moved < 5.0 * expected,
        "{moved} vs ~{expected}"
    );
}

#[test]
fn pushes_on_sample_times_and_at_the_start() {
    let (mut eph, _, _) = earth_moon();
    eph.extend_to(1e6);
    let on_sample = eph.times[5];
    let mut reference = eph.clone();
    let push = |start: f64| Push {
        body: 0,
        start,
        end: start + 100.0,
        force: DVec3::X * 1e18,
    };
    eph.push(push(on_sample));
    eph.extend_to(1e6);
    // The interval before the edge is unchanged; the edge is stored twice.
    let t = 0.5 * (eph.times[4] + on_sample);
    assert_eq!(eph.position(0, t), reference.position(0, t));
    assert_eq!(eph.times[5], eph.times[6]);
    reference.push(push(reference.start()));
    reference.extend_to(1e6);
    assert!(reference.position(0, 1e6).is_finite());
}

#[test]
fn many_tiny_pushes_add_up_exactly() {
    // A lone 3.7e22 kg planet coasting at 30 km/s, pushed tick by tick: each 1/60 s tick
    // adds 1e-14 m/s, 0.003 of the velocity's last bit. Every push restarts the integrator
    // from a stored sample; the compensation must survive that.
    let mu = 9.81 * 5e5 * 5e5;
    let mass = mu / G;
    let mut eph = Ephemeris::new(
        vec![body("Home", mu, 5e5)],
        vec![DVec3::ZERO],
        vec![DVec3::X * 3e4],
        0.0,
    );
    let dt = 1.0 / 60.0;
    let force = DVec3::X * (1e-14 / dt * mass);
    let ticks = 3600;
    for tick in 0..ticks {
        let t = f64::from(tick) * dt;
        eph.push(Push {
            body: 0,
            start: t,
            end: f64::from(tick + 1) * dt,
            force,
        });
        eph.extend_to(f64::from(tick + 1) * dt);
        eph.forget_before(t);
    }
    let (v, comp) = eph.velocity_with_compensation(0);
    let gained = (v.x - 3e4) - comp.x;
    let expected = 1e-14 * f64::from(ticks);
    assert!(
        (gained - expected).abs() < 1e-3 * expected,
        "gained {gained:e}, expected {expected:e}"
    );
}

#[test]
fn a_hierarchical_system_conserves_energy_and_momentum() {
    // Sun, a planet at 1.36e10 m and its moon at 1.2e7 m.
    let sun = 1.172_332_8e18;
    let planet = 9.81 * 5e5 * 5e5;
    let moon = 6.5e10;
    let (rp, vp) = kepler::Elements::from_semi_major_axis(sun, 1.36e10, 0.05, [0.0; 4]).to_state();
    let (rm, vm) =
        kepler::Elements::from_semi_major_axis(planet, 1.2e7, 0.02, [0.1, 0.0, 0.0, 1.0])
            .to_state();
    let bodies = vec![
        body("Sun", sun, 2.6e8),
        body("Home", planet, 5e5),
        body("Moon", moon, 2e5),
    ];
    let mut eph = Ephemeris::new(
        bodies,
        vec![DVec3::ZERO, rp, rp + rm],
        vec![DVec3::ZERO, vp, vp + vm],
        0.0,
    );
    let (e0, p0, l0) = (eph.energy(), eph.momentum(), eph.angular_momentum());
    // One year of the planet: ~ 230 orbits of the moon.
    eph.extend_to(9.2e6);
    let drift = ((eph.energy() - e0) / e0).abs();
    assert!(drift < 1e-13, "energy drift {drift:e}");
    let scale = sun / G * 1e3; // a momentum scale: the sun's mass at 1 km/s
    assert!((eph.momentum() - p0).length() < 1e-13 * scale);
    assert!((eph.angular_momentum() - l0).length() < 1e-13 * l0.length());
    println!(
        "{} samples per planet year, {} force evaluations",
        eph.sample_count(),
        eph.evaluations()
    );
}

/// Cross-platform determinism of the n-body integration (IAS15, Kahan sums, pushes).
/// Generated on Windows x86-64; never regenerate it to make a failure go away.
#[test]
fn the_bodies_move_identically_on_every_platform() {
    let mut eph = crate::testkit::system();
    eph.extend_to(10.0 * 86_400.0);
    eph.push(Push {
        body: 2,
        start: 10.0 * 86_400.0,
        end: 10.0 * 86_400.0 + 3600.0,
        force: DVec3::new(3e9, -1e9, 2e9),
    });
    eph.extend_to(30.0 * 86_400.0);
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for body in 0..eph.len() {
        let (x, v) = eph.state(body, 30.0 * 86_400.0);
        for value in [x.x, x.y, x.z, v.x, v.y, v.z] {
            hash = (hash ^ value.to_bits()).wrapping_mul(0x0100_0000_01b3);
        }
    }
    if std::env::var_os("CREEER_PRINT_GOLDEN").is_some() {
        println!("ephemeris golden hash: {hash:#018x}");
    }
    assert_eq!(hash, GOLDEN_EPHEMERIS_HASH, "got {hash:#018x}");
}

const GOLDEN_EPHEMERIS_HASH: u64 = 0x70a5_2fcd_9e9f_86d6;
