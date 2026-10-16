use super::*;
use crate::body::BodyKind;
use crate::ephemeris::Ephemeris;
use crate::kepler::Elements;
use crate::rails::{Group, Particle};
use crate::testkit;

const EARTH: f64 = 3.986_004_418e14;
const SUN: f64 = 1.327_124_400_18e20;

#[test]
fn hohmann_from_low_orbit_to_geostationary() {
    let (dv1, dv2, time) = hohmann(EARTH, 6.678e6, 4.2164e7);
    // 2.426 + 1.467 km/s; half the period of the 24,421 km transfer ellipse: 5.275 h.
    assert!((dv1 - 2426.0).abs() < 2.0, "{dv1}");
    assert!((dv2 - 1467.0).abs() < 2.0, "{dv2}");
    assert!((time / 3600.0 - 5.275).abs() < 0.001, "{time}");
    assert!((escape_speed(EARTH, 6.371e6) - 11_186.0).abs() < 1.0);
}

#[test]
fn earths_hill_sphere_soi_and_the_moons_roche_limit() {
    let a = 1.495_978_7e11;
    assert!((hill_radius(a, 0.0, EARTH, SUN) / 1.4966e9 - 1.0).abs() < 1e-3);
    assert!((sphere_of_influence(a, EARTH, SUN) / 9.246e8 - 1.0).abs() < 1e-3);
    // Rigid-body Roche limit of the Moon: 9,492 km.
    let d = roche_limit(6.371e6, 5514.0, 3344.0);
    assert!((d / 9.492e6 - 1.0).abs() < 1e-3, "{d}");
}

/// In the frame turning with a circular pair, all five points are equilibria.
#[test]
fn lagrange_points_balance_gravity_and_the_centrifugal_force() {
    for m in [1e-6, 0.012_15, 0.1, 0.3] {
        let (mu1, mu2) = (1.0 - m, m);
        let r = 1.0;
        // Circular orbit about the barycentre, angular velocity 1 (G(M+m) = r = 1).
        let x1 = DVec3::new(-m, 0.0, 0.0);
        let x2 = DVec3::new(1.0 - m, 0.0, 0.0);
        let v1 = DVec3::new(0.0, 0.0, m); // prograde about +Y: +X moves to −Z, so v ∥ −Z at +X
        let v2 = DVec3::new(0.0, 0.0, -(1.0 - m));
        let points = lagrange_points((x1, v1, mu1), (x2, v2, mu2));
        for (k, p) in points.iter().enumerate() {
            let pull = |x: DVec3, mu: f64| {
                let d = x - *p;
                d * (mu / (d.length_squared() * d.length()))
            };
            // Centrifugal: ω²·(distance from the axis through the barycentre).
            let net = pull(x1, mu1) + pull(x2, mu2) + *p;
            assert!(net.length() < 1e-12, "m = {m}, L{}: {net}", k + 1);
        }
        let l4 = points[3];
        assert!(((l4 - x1).length() - r).abs() < 1e-14);
        assert!(((l4 - x2).length() - r).abs() < 1e-14);
        // L4 leads: it lies ahead of the secondary in its direction of motion.
        assert!((l4 - x2).dot(v2) > 0.0, "m = {m}");
    }
}

#[test]
fn the_hierarchy_of_a_star_planet_and_moon() {
    let eph = testkit::system();
    let mu: Vec<f64> = eph.bodies().iter().map(|b| b.mu).collect();
    let x: Vec<DVec3> = (0..3).map(|i| eph.position(i, 0.0)).collect();
    let parents = primaries(&mu, &x);
    assert_eq!(parents, vec![None, Some(0), Some(1)]);
    assert_eq!(primary_of(&mu, &x, &parents, x[2] + DVec3::X * 3e5), 2);
    assert_eq!(primary_of(&mu, &x, &parents, x[1] + DVec3::X * 6e5), 1);
    assert_eq!(primary_of(&mu, &x, &parents, x[1] * 0.5), 0);
}

#[test]
fn corotating_coordinates_round_trip() {
    let a = (DVec3::new(1.0, 2.0, 3.0), DVec3::new(0.1, 0.0, -0.2));
    let b = (DVec3::new(5.0, -1.0, 2.0), DVec3::new(-0.3, 0.2, 0.4));
    let p = DVec3::new(-7.0, 4.0, 0.5);
    let back = from_corotating(to_corotating(p, a, b), a, b);
    assert!((back - p).length() < 1e-13);
    // The secondary sits on +x at its distance.
    let s = to_corotating(b.0, a, b);
    assert!((s - DVec3::X * (b.0 - a.0).length()).length() < 1e-13);
}

/// Routh's criterion: L4 is stable only for mass ratios below 0.0385. A test particle placed
/// near L4 librates about it for m = 0.001 and wanders off for m = 0.1.
#[test]
fn trojans_are_stable_only_below_routhes_mass_ratio() {
    for (m, stable) in [(0.001, true), (0.1, false)] {
        let mu = 1e18;
        let (mu1, mu2) = (mu * (1.0 - m), mu * m);
        let a = 1e10;
        let (r, w) = Elements::from_semi_major_axis(mu, a, 0.0, [0.0; 4]).to_state();
        let bodies = vec![
            testkit::body("Sun", BodyKind::Star, mu1, 1e8),
            testkit::body("Giant", BodyKind::GasGiant, mu2, 1e7),
        ];
        let x = vec![-r * m, r * (1.0 - m)];
        let v = vec![-w * m, w * (1.0 - m)];
        let mut eph = Ephemeris::new(bodies, x.clone(), v.clone(), 0.0);
        let l4 = lagrange_points((x[0], v[0], mu1), (x[1], v[1], mu2))[3];
        // Co-moving with L4 (rigid rotation about the barycentre), nudged by 1e-3 of a.
        let omega = (mu / (a * a * a)).sqrt();
        let axis = r.cross(w).normalize();
        let vel = axis.cross(l4) * omega;
        let mut trojan = Group::new(
            0.0,
            &[(l4 + DVec3::X * (1e-3 * a), vel)],
            vec![Particle { mass: 1.0, mu: 0.0 }],
        );
        let period = std::f64::consts::TAU / omega;
        let mut farthest = 0.0f64;
        for k in 1..=200 {
            let t = f64::from(k) * 0.25 * period;
            eph.extend_to(t);
            trojan.propagate(&eph, t, Some(0.0));
            let pair = (eph.state(0, t), eph.state(1, t));
            let here = to_corotating(trojan.state(0).0, pair.0, pair.1);
            let l4_now = to_corotating(
                lagrange_points((pair.0.0, pair.0.1, mu1), (pair.1.0, pair.1.1, mu2))[3],
                pair.0,
                pair.1,
            );
            farthest = farthest.max((here - l4_now).length() / a);
        }
        println!("m = {m}: farthest {farthest:.4} of a from L4 in 50 orbits");
        if stable {
            assert!(farthest < 0.05, "m = {m}: {farthest}");
        } else {
            assert!(farthest > 0.3, "m = {m}: {farthest}");
        }
    }
}
