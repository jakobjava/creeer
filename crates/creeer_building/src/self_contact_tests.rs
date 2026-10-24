//! Members of one structure colliding with each other (G2.5, ADR-0034): a frame folding on
//! itself is stopped by its own members' prisms, as two bodies' would be.

use crate::assembly::{Fastening, Motion, Part, Profile, assemble};
use crate::assembly_tests::spruce;
use creeer_math::glam::DVec3;
use creeer_physics::structure::{JointLaw, Material};
use creeer_physics::world::shape::StaticShape;
use creeer_physics::world::{Body, World};
use std::time::Instant;

const DT: f64 = 1.0 / 60.0;

fn wood() -> Material {
    Material {
        density: 800.0,
        ..spruce()
    }
}

/// A pole of 50 mm from `start` to `end`, turning with `motion`.
fn pole(start: DVec3, end: DVec3, motion: Motion) -> Part {
    Part {
        start,
        end,
        up: DVec3::Y,
        profile: Profile::Round {
            start: 0.05,
            end: 0.05,
        },
        material: wood(),
        motion,
    }
}

/// A nail's law, its turning soft (a hinge that holds the corner together and lets it fold).
fn hinge() -> JointLaw {
    JointLaw {
        axial: 1.5e7,
        bearing: 4500.0,
        withdrawal: 700.0,
        lateral: 9.6e5,
        lateral_yield: 1000.0,
        lateral_peak: 1160.0,
        slip_at_peak: 0.01,
        slip_at_failure: 0.02,
        twist: 0.5,
        twist_yield: 0.2,
        interaction: 1.0,
        damping: 0.01,
    }
}

/// A V of two 1 m poles hinged at the corner (the origin), 40° apart in the horizontal
/// plane, their arms swung towards each other at `spin` rad/s each, weightless: the least
/// distance between their outer halves' axes over two seconds, and the frame.
fn fold(spin: f64, falls: bool) -> (f64, Body) {
    let half = 20f64.to_radians();
    let (sin, cos) = creeer_math::det::sin_cos(half);
    let arm = |sign: f64| DVec3::new(cos, 0.0, sign * sin);
    let motion = |sign: f64| Motion {
        centre: DVec3::ZERO,
        velocity: DVec3::ZERO,
        spin: DVec3::Y * (sign * spin),
    };
    let parts = [
        pole(DVec3::ZERO, arm(1.0), motion(1.0)),
        // The second arm lapped on the first at the corner (the nail upright: the hinge's
        // axis), sloping down to the first's height at its end.
        pole(DVec3::Y * 0.05, arm(-1.0), motion(-1.0)),
    ];
    let joint = Fastening {
        a: 0,
        b: 1,
        on_a: 0.0,
        on_b: 0.0,
        law: hinge(),
    };
    let built = assemble(&parts, &[joint]).expect("builds");
    let mut body = Body::structure(built.structure, DT);
    body.shapes = built.shapes;
    let mut w = World::new(DT);
    if falls {
        w.statics.push(StaticShape::Plane {
            normal: DVec3::Y,
            offset: -0.3,
        });
    } else {
        body.structure.gravity = DVec3::ZERO;
    }
    w.add(body);
    let (a, b) = (built.nodes[0].clone(), built.nodes[1].clone());
    let mut least = f64::INFINITY;
    for _ in 0..120 {
        w.step();
        let s = &w.bodies[0].structure;
        // The outer halves' nodes, compared across the arms.
        for &(sa, na) in a.iter().filter(|n| n.0 > 0.5) {
            for &(sb, nb) in b.iter().filter(|n| n.0 > 0.5) {
                if (sa - sb).abs() < 0.2 {
                    least = least.min((s.position[na] - s.position[nb]).length());
                }
            }
        }
    }
    (least, w.bodies.swap_remove(0))
}

/// Folding shut, the arms stop on each other: their axes stay a pole's diameter apart (50 mm,
/// less what the faces press in), where before G2.5 they passed through each other.
#[test]
fn a_frame_folding_shut_stops_on_itself() {
    let (least, body) = fold(1.0, false);
    assert!(
        body.structure.position.iter().all(|p| p.is_finite()),
        "{least}"
    );
    assert!(least > 0.045, "the arms came {least} m apart");
}

/// A frame whose corner holds no turn (its joint a hinge, as a broken one would leave it)
/// falling 0.3 m onto the ground as it folds: it lands and folds shut on itself, its arms never
/// through each other.
#[test]
fn a_collapsing_frame_does_not_fold_through_itself() {
    let (least, body) = fold(1.5, true);
    let s = &body.structure;
    assert!(
        s.position.iter().all(|p| p.is_finite() && p.y > -0.3),
        "{least}"
    );
    assert!(least > 0.045, "the arms came {least} m apart");
}

/// The cost of self-contact for a frame at rest: twenty poles nailed into a ladder lying on
/// the ground, stepped for 2 s (ms, printed; measured beside the same with no self-contact
/// before G2.5 in ADR-0034).
#[test]
fn a_ladder_of_twenty_poles_rests_on_the_ground() {
    let mut parts = Vec::new();
    let mut fastenings = Vec::new();
    for side in [0.0, 0.6] {
        parts.push(pole(
            DVec3::new(0.0, 0.025, side),
            DVec3::new(4.0, 0.025, side),
            Motion::default(),
        ));
    }
    for k in 0..18 {
        let x = 0.1 + 0.22 * f64::from(k);
        parts.push(pole(
            DVec3::new(x, 0.075, -0.1),
            DVec3::new(x, 0.075, 0.7),
            Motion::default(),
        ));
        for (rail, on) in [(0, 0.1), (1, 0.7)] {
            fastenings.push(Fastening {
                a: rail,
                b: parts.len() - 1,
                on_a: x,
                on_b: on,
                law: hinge(),
            });
        }
    }
    let built = assemble(&parts, &fastenings).expect("builds");
    let mut body = Body::structure(built.structure, DT);
    body.shapes = built.shapes;
    let mut w = World::new(DT);
    w.statics.push(StaticShape::Plane {
        normal: DVec3::Y,
        offset: 0.0,
    });
    w.add(body);
    let t = Instant::now();
    for _ in 0..120 {
        w.step();
    }
    let ms = t.elapsed().as_secs_f64() * 1e3;
    println!(
        "ladder of 20 poles: {ms:.1} ms for 120 steps, {} contacts, asleep {}",
        w.contacts.len(),
        w.bodies[0].sleep.asleep
    );
    let s = &w.bodies[0].structure;
    assert!(s.position.iter().all(|p| p.is_finite() && p.y.abs() < 0.2));
}
