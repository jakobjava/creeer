//! Hulls in motion: a wedge slides by Coulomb's law, a hexagonal prism tips over its edge,
//! and a heap of rocks settles.

use super::contact::STICK_DISTANCE;
use super::hull::Hull;
use super::hull_contact_tests::{deepest, numbers};
use super::hull_sat::hull_hull;
use super::hull_tests::cloud;
use super::narrow::pose;
use super::shape::{Shape, StaticShape};
use super::*;
use creeer_math::det;
use std::sync::Arc;

const DT: f64 = 1.0 / 60.0;
const G: f64 = 9.806_65;

/// A body of `shape` (`mass` kg) on a ramp tilted by `degrees`, turned by `yaw` about its
/// own up axis, friction μ on both surfaces, set down on it at `lift` above its centre; how
/// far it slid down the ramp between t = 1 s and 2 s.
fn ramp(shape: Shape, mass: f64, (lift, yaw): (f64, f64), degrees: f64, mu: f64) -> f64 {
    let angle = degrees.to_radians();
    let (sin, cos) = det::sin_cos(angle);
    let normal = DVec3::new(-sin, cos, 0.0);
    let down = DVec3::new(-cos, -sin, 0.0);
    let mut world = World::new(DT);
    world.statics.push(StaticShape::Plane {
        normal,
        offset: 0.0,
    });
    world.static_friction = mu;
    let mut body = Body::rigid(shape, mass, normal * lift, DT);
    body.structure.orientation[0] = rotation(DVec3::Z, angle) * rotation(DVec3::Y, yaw);
    body.friction = mu;
    world.add(body);
    let along = |w: &World| w.bodies[0].structure.position[0].dot(down);
    for _ in 0..60 {
        world.step();
    }
    let start = along(&world);
    for _ in 0..60 {
        world.step();
    }
    along(&world) - start
}

/// A wedge (a triangular prism lying on its rectangular face, its tall end uphill) on a ramp
/// obeys Coulomb's law as a box does: below the friction angle it stays; above it, it
/// slides with a = g·(sin θ − μ·cos θ), within 2 %. (Turned round, friction below its centre
/// of mass moves the pressure's resultant out of the middle third of its base, part of the
/// face lifts, and it slides 2 % slow, as tall boxes do: ADR-0005.)
#[test]
fn a_wedge_on_a_ramp_follows_coulomb() {
    let points = [
        DVec3::new(-0.5, 0.0, -0.3),
        DVec3::new(0.5, 0.0, -0.3),
        DVec3::new(-0.5, 0.0, 0.3),
        DVec3::new(0.5, 0.0, 0.3),
        DVec3::new(-0.5, 0.5, -0.3),
        DVec3::new(-0.5, 0.5, 0.3),
    ];
    let (hull, centroid) = Hull::new(&points).expect("a wedge");
    let wedge = || Shape::Hull(Arc::new(hull.clone()));
    let set = (centroid.y, std::f64::consts::PI);
    let stuck = ramp(wedge(), 100.0, set, 20.0, 0.6);
    assert!(
        stuck.abs() < 1e-4,
        "slid {stuck} m below the friction angle"
    );
    let (sin, cos) = det::sin_cos(40f64.to_radians());
    let want = 0.5 * G * (sin - 0.3 * cos) * 3.0;
    let slid = ramp(wedge(), 100.0, set, 40.0, 0.3);
    assert!(
        (slid / want - 1.0).abs() < 0.02,
        "slid {slid} m, expected {want} m"
    );
}

/// A hexagonal prism lying on a ramp steeper than 30° tips over its lower edge: it turns a
/// sixth of a turn about that edge, which its friction (μ = 0.9 > tan 35°) holds in place
/// along the ramp within its friction's stick distance (0.1 mm) over the first three quarters
/// of the turn (in the
/// last its spin unloads the edge, which slips as its load falls to nothing and leaves the
/// ramp, as a body tipping fast leaves its edge), and lands on its next face. (Before the corner forces of an unevenly pressed face reached the bodies,
/// F5, the edge slipped 0.7 mm in its first two steps.)
#[test]
fn a_hexagonal_prism_tips_over_its_lower_edge() {
    let mut points = Vec::new();
    for k in 0..6 {
        let angle = std::f64::consts::TAU * f64::from(k) / 6.0;
        for z in [-0.4, 0.4] {
            points.push(DVec3::new(det::cos(angle) * 0.3, det::sin(angle) * 0.3, z));
        }
    }
    let (hull, _) = Hull::new(&points).expect("a prism");
    let angle = 35f64.to_radians();
    let (sin, cos) = det::sin_cos(angle);
    let normal = DVec3::new(-sin, cos, 0.0);
    let mut world = World::new(DT);
    world.statics.push(StaticShape::Plane {
        normal,
        offset: 0.0,
    });
    world.static_friction = 0.9;
    // Lying on the face between its corners at 240° and 300°: its centre 0.3·cos 30° above
    // it, the corner at 240° on its downhill edge (local x runs uphill).
    let mut body = Body::rigid(
        Shape::Hull(Arc::new(hull)),
        50.0,
        normal * (0.3 * 0.75f64.sqrt()),
        DT,
    );
    body.structure.orientation[0] = rotation(DVec3::Z, angle);
    body.friction = 0.9;
    world.add(body);
    let edge = |w: &World| {
        let s = &w.bodies[0].structure;
        let corner = DVec3::new(-0.15, -0.3 * 0.75f64.sqrt(), 0.4);
        [
            s.position[0] + s.orientation[0] * corner,
            s.position[0] + s.orientation[0] * (corner * DVec3::new(1.0, 1.0, -1.0)),
        ]
    };
    // Its first step, set down with no load, its friction starts from none (lagged): measured
    // from after it.
    world.step();
    let mut turned = world.bodies[0].structure.angular_velocity[0].z * DT;
    let start = edge(&world);
    let mut drift = 0.0f64;
    for _ in 0..90 {
        world.step();
        turned += world.bodies[0].structure.angular_velocity[0].z * DT;
        if turned >= std::f64::consts::PI / 3.0 - 0.01 {
            break;
        }
        for (now, then) in edge(&world).iter().zip(&start) {
            let moved = *now - *then;
            if turned < 0.75 * std::f64::consts::PI / 3.0 {
                drift = drift.max((moved - normal * moved.dot(normal)).length());
            }
        }
    }
    assert!(
        turned >= std::f64::consts::PI / 3.0 - 0.01,
        "turned only {turned} rad"
    );
    assert!(drift < STICK_DISTANCE, "its edge slipped {drift} m");
}

/// Rocks (hulls of ten random points on a flattened ball) dropped in a heap come to rest on
/// the ground and on each other: never more energy than they started with (but for
/// Newmark's bounded oscillation, as for a tumbling box), none sunk into the ground or into
/// another by more than 2 mm, all but still at the end. (A rock on a sharp corner sinks
/// about a millimetre where Hertz on its rounding would give tens of micrometres: a face
/// pressed near a corner follows the punch law of its whole face, ADR-0005.)
#[test]
fn a_heap_of_rocks_settles_without_gaining_energy() {
    let mut world = World::new(DT);
    world.statics.push(StaticShape::Plane {
        normal: DVec3::Y,
        offset: 0.0,
    });
    let mut next = numbers(5);
    for k in 0..12u32 {
        let flat: Vec<DVec3> = cloud(10, 0.3, u64::from(k) + 1)
            .into_iter()
            .map(|p| p * DVec3::new(1.0, 0.7, 0.45))
            .collect();
        let (hull, _) = Hull::new(&flat).expect("a rock");
        let at = DVec3::new(next() - 0.5, 0.5 + 0.7 * f64::from(k), next() - 0.5);
        let mass = 2600.0 * hull.volume();
        let mut rock = Body::rigid(Shape::Hull(Arc::new(hull)), mass, at, DT);
        rock.structure.orientation[0] = rotation(DVec3::new(next(), next(), next()), 3.0 * next());
        rock.surface = surface::Surface::ROCK;
        rock.friction = 0.7;
        world.add(rock);
    }
    // The energy is measured from where they start; the scale of what goes on is the
    // potential energy the fall releases.
    let start = world.energy();
    let released: f64 = world
        .bodies
        .iter()
        .map(|b| b.structure.masses()[0] * G * b.structure.position[0].y)
        .sum();
    for tick in 0..600 {
        world.step();
        let now = world.energy();
        assert!(
            now <= start + 1e-3 * released,
            "tick {tick}: {now} > {start}"
        );
    }
    let placed: Vec<(Arc<Hull>, DVec3, DQuat)> = world
        .bodies
        .iter()
        .map(|b| {
            let Shape::Hull(h) = &b.shapes[0].1 else {
                unreachable!()
            };
            (
                h.clone(),
                b.structure.position[0],
                b.structure.orientation[0],
            )
        })
        .collect();
    for (k, (hull, p, q)) in placed.iter().enumerate() {
        let lowest = hull
            .vertices()
            .iter()
            .map(|&v| (*p + *q * v).y)
            .fold(f64::INFINITY, f64::min);
        assert!(lowest > -2e-3, "rock {k} {lowest} m into the ground");
        assert!(
            world.bodies[k].structure.velocity[0].length() < 1e-2,
            "rock {k} still moving"
        );
        for (j, (other, po, qo)) in placed.iter().enumerate().skip(k + 1) {
            let deepest = deepest(&hull_hull(pose(*p, *q), hull, pose(*po, *qo), other, 0.0));
            assert!(
                deepest > -2e-3,
                "rocks {k} and {j} {} m into each other",
                -deepest
            );
        }
    }
}
