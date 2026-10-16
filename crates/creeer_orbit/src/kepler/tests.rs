use super::*;
use proptest::prelude::*;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

const MU: f64 = 3.986_004_418e14; // Earth (m³/s²)

fn close(a: DVec3, b: DVec3, tolerance: f64) -> bool {
    (a - b).length() <= tolerance
}

#[test]
fn stumpff_series_meets_the_closed_forms() {
    // Where the closed forms take over, the series agrees with them to rounding.
    for z in [-2.0, -1.0, 1.0, 2.0] {
        let (c, s) = stumpff(z);
        let (c_series, s_series) = stumpff_series(z);
        assert!((c - c_series).abs() < 4e-16, "C at {z}: {c} vs {c_series}");
        assert!((s - s_series).abs() < 4e-16, "S at {z}: {s} vs {s_series}");
    }
    assert_eq!(stumpff(0.0), (0.5, 1.0 / 6.0));
    // C(π²) = 2/π² (cos π = −1), S(4π²) = 1/(4π²).
    let (c, _) = stumpff(PI * PI);
    assert!((c - 2.0 / (PI * PI)).abs() < 1e-16);
    let (_, s) = stumpff(4.0 * PI * PI);
    assert!((s - 1.0 / (4.0 * PI * PI)).abs() < 1e-16);
}

/// Vallado, *Fundamentals of Astrodynamics and Applications* (4th ed.), example 2-4.
#[test]
fn matches_vallados_worked_example() {
    let r0 = DVec3::new(1131.340, -2282.343, 6672.423) * 1e3;
    let v0 = DVec3::new(-5.64305, 4.30333, 2.42879) * 1e3;
    let (r, v) = propagate(MU, r0, v0, 40.0 * 60.0);
    let r_expected = DVec3::new(-4219.7527, 4363.0292, -3958.7666) * 1e3;
    let v_expected = DVec3::new(3.689866, -1.916735, -6.112511) * 1e3;
    // The book prints 4 decimals of km and 6 of km/s.
    assert!(close(r, r_expected, 0.2), "{r} vs {r_expected}");
    assert!(close(v, v_expected, 2e-3), "{v} vs {v_expected}");
}

#[test]
fn circular_orbit_turns_by_its_mean_motion() {
    let radius = 7.0e6;
    let speed = (MU / radius).sqrt();
    let period = TAU * radius / speed;
    let (r, v) = propagate(MU, DVec3::X * radius, DVec3::NEG_Z * speed, period / 4.0);
    // Prograde about +Y: +X moves towards −Z.
    assert!(close(r, DVec3::NEG_Z * radius, 1e-6), "{r}");
    assert!(close(v, DVec3::NEG_X * speed, 1e-9), "{v}");
    // A thousand whole revolutions plus a quarter: the same place, to rounding.
    let (r, _) = propagate(
        MU,
        DVec3::X * radius,
        DVec3::NEG_Z * speed,
        1000.25 * period,
    );
    assert!(close(r, DVec3::NEG_Z * radius, 1e-4), "{r}");
}

#[test]
fn elements_of_a_known_orbit() {
    // 300 × 3000 km altitudes about a 6378 km Earth.
    let (rp, ra) = (6.678e6, 9.378e6);
    let a = 0.5 * (rp + ra);
    let e = (ra - rp) / (ra + rp);
    let elements = Elements::from_semi_major_axis(MU, a, e, [0.3, 1.0, 2.0, 0.0]);
    assert!((elements.periapsis() - rp).abs() < 1e-6);
    assert!((elements.apoapsis() - ra).abs() < 1e-6);
    let (r, v) = elements.to_state();
    assert!((r.length() - rp).abs() < 1e-6);
    // Vis-viva at periapsis.
    let vp = (MU * (2.0 / rp - 1.0 / a)).sqrt();
    assert!((v.length() - vp).abs() < 1e-9);
    assert!((elements.period() - TAU * (a * a * a / MU).sqrt()).abs() < 1e-9);
    assert!((elements.energy() - (0.5 * vp * vp - MU / rp)).abs() < 1e-6);
}

#[test]
fn prograde_equatorial_orbits_have_zero_inclination_and_y_momentum() {
    let (r, v) = (DVec3::X * 7e6, DVec3::NEG_Z * 7.6e3);
    assert!(r.cross(v).y > 0.0);
    let elements = Elements::from_state(MU, r, v);
    assert!(elements.inclination.abs() < 1e-15);
    let (r, v) = (DVec3::X * 7e6, DVec3::Z * 7.6e3);
    assert!((Elements::from_state(MU, r, v).inclination - PI).abs() < 1e-15);
    let polar = Elements::from_state(MU, DVec3::X * 7e6, DVec3::Y * 7.6e3);
    assert!((polar.inclination - FRAC_PI_2).abs() < 1e-15);
}

#[test]
fn mean_anomaly_advances_uniformly() {
    for e in [0.0, 0.1, 0.5, 0.9, 0.99] {
        let given = Elements::from_semi_major_axis(MU, 2.0e7, e, [0.4, 0.5, 0.6, 0.7]);
        let (r0, v0) = given.to_state();
        // A circular orbit has no periapsis: its angles are measured from the node.
        let elements = Elements::from_state(MU, r0, v0);
        let n = TAU / elements.period();
        let dt = 0.37 * elements.period();
        let (r, v) = propagate(MU, r0, v0, dt);
        let later = Elements::from_state(MU, r, v);
        let advanced = (later.mean_anomaly() - elements.mean_anomaly()).rem_euclid(TAU);
        assert!((advanced - n * dt).abs() < 1e-10, "e = {e}: {advanced}");
        // And the mean anomaly turns back into the same true anomaly.
        let back = elements.with_mean_anomaly(elements.mean_anomaly());
        assert!(
            (back.true_anomaly - elements.true_anomaly).abs() < 1e-12,
            "e = {e}"
        );
    }
}

#[test]
fn hyperbolic_mean_anomaly_round_trips() {
    let elements = Elements::from_semi_major_axis(MU, -1.5e7, 1.8, [0.2, 0.1, 0.3, 1.2]);
    let back = elements.with_mean_anomaly(elements.mean_anomaly());
    assert!((back.true_anomaly - elements.true_anomaly).abs() < 1e-12);
    // M advances by n·t with n = √(μ/(−a)³).
    let (r0, v0) = elements.to_state();
    let a = elements.semi_major_axis();
    let n = (MU / (-a * a * a)).sqrt();
    let (r, v) = propagate(MU, r0, v0, 3600.0);
    let later = Elements::from_state(MU, r, v);
    let advanced = later.mean_anomaly() - elements.mean_anomaly();
    assert!((advanced - n * 3600.0).abs() < 1e-10);
}

#[test]
fn parabolic_escape_keeps_zero_energy() {
    let r0 = DVec3::X * 7e6;
    let v0 = DVec3::NEG_Z * (2.0 * MU / 7e6).sqrt();
    let (r, v) = propagate(MU, r0, v0, 20_000.0);
    let energy = 0.5 * v.length_squared() - MU / r.length();
    assert!(energy.abs() < 1e-6, "{energy}");
    assert!(r.length() > 5e7);
}

#[test]
fn asinh_is_the_inverse_of_sinh() {
    for x in [-30.0, -1.0, -1e-9, 0.0, 1e-12, 0.5, 3.0, 700.0] {
        assert!(
            (asinh(det::sinh(x)) - x).abs() <= 1e-15 * x.abs().max(1.0),
            "{x}"
        );
    }
}

fn arb_state() -> impl Strategy<Value = (DVec3, DVec3)> {
    // Radii 6.6e6..4e7 m, speeds from well below circular to well above escape.
    (
        6.6e6..4e7f64,
        0.0..PI,
        0.0..TAU,
        0.2..1.6f64,
        0.0..PI,
        0.0..TAU,
    )
        .prop_map(|(r, theta, phi, speed, vt, vp)| {
            let dir = |t: f64, p: f64| {
                let (st, ct) = det::sin_cos(t);
                let (sp, cp) = det::sin_cos(p);
                DVec3::new(st * cp, ct, st * sp)
            };
            let circular = (MU / r).sqrt();
            (dir(theta, phi) * r, dir(vt, vp) * circular * speed)
        })
        .prop_filter("radial orbits have no plane", |(r, v)| {
            r.normalize().cross(v.normalize()).length() > 1e-3
        })
}

proptest! {
    #[test]
    fn propagation_conserves_energy_and_momentum((r0, v0) in arb_state(), dt in -2e5..2e5f64) {
        let (r, v) = propagate(MU, r0, v0, dt);
        let energy = |r: DVec3, v: DVec3| 0.5 * v.length_squared() - MU / r.length();
        let e0 = energy(r0, v0);
        let scale = MU / r0.length();
        prop_assert!((energy(r, v) - e0).abs() < 1e-9 * scale, "energy {} vs {}", energy(r, v), e0);
        let h0 = r0.cross(v0);
        prop_assert!((r.cross(v) - h0).length() < 1e-9 * h0.length());
    }

    #[test]
    fn propagation_runs_backwards((r0, v0) in arb_state(), dt in 1.0..1e5f64) {
        let (r, v) = propagate(MU, r0, v0, dt);
        let (back, back_v) = propagate(MU, r, v, -dt);
        let scale = r0.length().max(r.length());
        prop_assert!((back - r0).length() < 1e-8 * scale, "{back} vs {r0}");
        prop_assert!((back_v - v0).length() < 1e-8 * v0.length().max(1.0));
    }

    #[test]
    fn steps_compose((r0, v0) in arb_state(), a in 0.0..5e4f64, b in 0.0..5e4f64) {
        let (r1, v1) = propagate(MU, r0, v0, a);
        let (r2, _) = propagate(MU, r1, v1, b);
        let (direct, _) = propagate(MU, r0, v0, a + b);
        prop_assert!((r2 - direct).length() < 1e-7 * r2.length().max(direct.length()));
    }

    #[test]
    fn elements_round_trip((r, v) in arb_state()) {
        let elements = Elements::from_state(MU, r, v);
        let (r2, v2) = elements.to_state();
        prop_assert!((r2 - r).length() < 1e-9 * r.length(), "{r2} vs {r}");
        prop_assert!((v2 - v).length() < 1e-9 * v.length());
    }
}
