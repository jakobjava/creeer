use crate::body::{BodyKind, G};
use crate::majors::{self, Push};
use crate::radau::{Forces, Radau};
use crate::rails::{Group, Particle};
use crate::testkit::{self, HOME_MU, HOME_RADIUS};
use creeer_math::det;
use creeer_math::glam::DVec3;

/// A ship (no gravity of its own) at `state` relative to Home at time 0.
fn ship_near_home(rel: (DVec3, DVec3)) -> (crate::Ephemeris, Group) {
    let eph = testkit::system();
    let (home_x, home_v) = eph.state(1, 0.0);
    let group = Group::new(
        0.0,
        &[(home_x + rel.0, home_v + rel.1)],
        vec![Particle { mass: 1e4, mu: 0.0 }],
    );
    (eph, group)
}

/// All three bodies and the ship in one integrator.
struct Everything(Vec<crate::Body>);

impl Forces for Everything {
    fn accelerations(&mut self, _t: f64, x: &[DVec3], _v: &[DVec3], a: &mut [DVec3]) {
        majors::gravity(&self.0, x, a);
    }
}

#[test]
fn rails_agree_with_integrating_everything_together() {
    let r = HOME_RADIUS + 1e5;
    let rel = (DVec3::X * r, DVec3::NEG_Z * (HOME_MU / r).sqrt());
    let (mut eph, mut group) = ship_near_home(rel);
    let day = 86_400.0;
    eph.extend_to(day);
    assert!(group.propagate(&eph, day, Some(0.0)).is_none());

    let reference = testkit::system();
    let mut bodies = reference.bodies().to_vec();
    bodies.push(testkit::body("Ship", BodyKind::Asteroid, 0.0, 0.0));
    let (mut x, mut v) = (Vec::new(), Vec::new());
    for i in 0..3 {
        let (xi, vi) = reference.state(i, 0.0);
        x.push(xi);
        v.push(vi);
    }
    x.push(x[1] + rel.0);
    v.push(v[1] + rel.1);
    let mut all = Radau::new(0.0, x, v, 10.0);
    all.integrate_to(&mut Everything(bodies), day, |_| {});
    let error = (group.state(0).0 - all.x[3]).length();
    println!("ship after a day (54 orbits): {error:.2e} m from the joint integration");
    assert!(error < 1e-3, "{error} m");
}

#[test]
fn a_fall_stops_at_the_surface_at_the_right_time() {
    // Released at rest relative to Home, 100 km up: radial free fall.
    let r0 = HOME_RADIUS + 1e5;
    let (mut eph, mut group) = ship_near_home((DVec3::Y * r0, DVec3::ZERO));
    eph.extend_to(1000.0);
    let hit = group
        .propagate(&eph, 1000.0, Some(0.0))
        .expect("it hits the ground");
    assert_eq!(hit.body, 1);
    // Radial Kepler fall from r0 to R: t = √(r0³/2μ)·(√(x(1−x)) + acos √x), x = R/r0.
    let x = HOME_RADIUS / r0;
    let expected =
        (r0 * r0 * r0 / (2.0 * HOME_MU)).sqrt() * ((x * (1.0 - x)).sqrt() + det::acos(x.sqrt()));
    assert!((hit.t - expected).abs() < 1e-3, "{} vs {expected} s", hit.t);
    let altitude = (group.state(0).0 - eph.position(1, hit.t)).length() - HOME_RADIUS;
    assert!(
        altitude.abs() < 1e-6,
        "stopped {altitude} m from the surface"
    );
    assert_eq!(group.time(), hit.t);
}

#[test]
fn a_push_changes_the_velocity_by_its_impulse() {
    // Far from Home: in low orbit the two paths (150 m apart by the end) would feel a
    // gravity gradient that changes the velocities by 1e-4 of the impulse.
    let (mut eph, mut pushed) = ship_near_home((DVec3::X * 3e9, DVec3::ZERO));
    let mut coasting = pushed.clone();
    eph.extend_to(200.0);
    let force = DVec3::new(0.0, 3e4, -1e4);
    pushed.push(Push {
        body: 0,
        start: 50.0,
        end: 60.0,
        force,
    });
    pushed.propagate(&eph, 60.0, Some(0.0));
    coasting.propagate(&eph, 60.0, Some(0.0));
    let dv = pushed.state(0).1 - coasting.state(0).1;
    let expected = force / 1e4 * 10.0;
    assert!(
        (dv - expected).length() < 1e-9 * expected.length(),
        "{dv} vs {expected}"
    );
}

#[test]
fn a_gravity_tractor_pulls_its_asteroid() {
    // A 1e10 kg asteroid and a 20 t ship 100 m from it, both in orbit about the Sun.
    let eph_start = testkit::system();
    let (home_x, home_v) = eph_start.state(1, 0.0);
    let far = DVec3::new(3e9, 0.0, 0.0); // well away from Home
    let asteroid = Particle {
        mass: 1e10,
        mu: G * 1e10,
    };
    let ship = Particle {
        mass: 2e4,
        mu: G * 2e4,
    };
    let start = (home_x + far, home_v);
    let offset = DVec3::Y * 100.0;
    let mut pair = Group::new(
        0.0,
        &[start, (start.0 + offset, start.1)],
        vec![asteroid, ship],
    );
    let mut alone = Group::new(0.0, &[start], vec![asteroid]);
    let mut eph = eph_start;
    let t = 30.0;
    eph.extend_to(t);
    pair.propagate(&eph, t, Some(0.0));
    alone.propagate(&eph, t, Some(0.0));
    // The ship's pull on the asteroid: μ_ship/r²·t, with r closing by 3 cm in 30 s (1/r²
    // averaged over the two ends). The pull is 4e-9 m/s on a heliocentric 3e4 m/s: a
    // thousand times the last bit of the velocity.
    let r_end = (pair.state(1).0 - pair.state(0).0).length();
    let expected = ship.mu * 0.5 * (1.0 / 1e4 + 1.0 / (r_end * r_end)) * t;
    let pull = (pair.state(0).1 - alone.state(0).1).y;
    assert!(
        (pull - expected).abs() < 3e-3 * expected,
        "{pull:e} vs {expected:e}"
    );
    // Third law: the ship gains the opposite momentum (relative to the free asteroid).
    let ship_gain = pair.state(1).1.y - alone.state(0).1.y;
    let momentum = asteroid.mass * pull + ship.mass * ship_gain;
    assert!(
        // The Sun's gradient over the 100 m between them adds 1e-6 of the gain.
        momentum.abs() < 1e-5 * ship.mass * ship_gain.abs(),
        "momentum {momentum:e}"
    );
}
