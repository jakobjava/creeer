//! Members colliding as prisms (G2.3): faces (hulls, boxes) on the nodes of multi-node
//! structures, resting on the ground as rigid bodies' faces do (ADR-0005, amended in G2.3).

use crate::assembly::{Motion, Part, Profile, assemble};
use crate::assembly_tests::spruce;
use creeer_math::glam::{DMat3, DVec3};
use creeer_physics::structure::{JointLaw, Material, Section, StructureBuilder};
use creeer_physics::world::hull::Hull;
use creeer_physics::world::shape::{Shape, StaticShape};
use creeer_physics::world::{Body, World};
use std::sync::Arc;

const DT: f64 = 1.0 / 60.0;

/// Spruce at a fresh pole's density (a survival build's).
fn wood() -> Material {
    Material {
        density: 800.0,
        ..spruce()
    }
}

/// The lowest point of a body's hulls (m).
fn lowest(b: &Body) -> f64 {
    let s = &b.structure;
    (b.shapes.iter())
        .filter_map(|(node, shape)| match shape {
            Shape::Hull(h) => Some((*node, h)),
            _ => None,
        })
        .flat_map(|(n, h)| {
            (h.vertices().iter()).map(move |v| (s.position[n] + s.orientation[n] * *v).y)
        })
        .fold(f64::INFINITY, f64::min)
}

/// Steps `body` alone on the ground plane for four seconds (the structures measured asleep
/// after about one, the rigid body after 2.7); returns it, asleep or not.
fn settle(body: Body) -> (Body, bool) {
    let mut w = World::new(DT);
    w.statics.push(StaticShape::Plane {
        normal: DVec3::Y,
        offset: 0.0,
    });
    w.add(body);
    for k in 0..240 {
        w.step();
        let b = &w.bodies[0];
        // (Before G2.3's fix the first step ran off to 10¹⁵⁰ m.)
        assert!(
            (b.structure.position.iter()).all(|p| p.is_finite() && p.y.abs() < 1.0),
            "step {k}: {:?}",
            b.structure.position
        );
    }
    let asleep = w.bodies[0].sleep.asleep;
    (w.bodies.swap_remove(0), asleep)
}

/// The eight corners of a box of half extents `half` centred at `at`.
fn box_points(half: DVec3, at: DVec3) -> Vec<DVec3> {
    (0..8)
        .map(|c| {
            let sign = |bit: i32| if c & bit == 0 { -1.0 } else { 1.0 };
            at + half * DVec3::new(sign(1), sign(2), sign(4))
        })
        .collect()
}

/// A hull of `points` given about its node.
fn hull(points: &[DVec3]) -> Shape {
    let (h, centroid) = Hull::new(points).expect("a hull");
    Shape::Hull(Arc::new(h.about(-centroid)))
}

/// A two-node beam 0.9 m long along x, 1 mm above the ground with boxes 50 mm tall on its
/// nodes, their centres `inwards` towards the middle of the beam.
fn beam_with_boxes(inwards: f64) -> Body {
    let mut sb = StructureBuilder::new();
    let a = sb.node(DVec3::new(-0.45, 0.026, 0.0));
    let b = sb.node(DVec3::new(0.45, 0.026, 0.0));
    sb.beam(a, b, Section::solid_round(0.05), wood());
    let mut body = Body::structure(sb.build(), DT);
    let half = DVec3::new(0.2, 0.025, 0.025);
    body.shapes = vec![
        (a, hull(&box_points(half, DVec3::X * inwards))),
        (b, hull(&box_points(half, -DVec3::X * inwards))),
    ];
    body
}

/// Where a face rests: pressed in by its weight at most a fraction of a millimetre (the
/// step's resolvable stiffness, `cap.rs`: micrometres), never lifted off.
fn rests_on_the_ground(b: &Body, what: &str) {
    let depth = -lowest(b);
    assert!(
        (0.0..1e-4).contains(&depth),
        "{what}: pressed in {depth:e} m"
    );
}

/// A pole as one member with half-prism hulls on its two nodes (`assemble`'s shapes).
#[test]
fn a_pole_with_prism_hulls_comes_to_rest_on_the_ground() {
    let p = Part {
        start: DVec3::new(-0.45, 0.026, 0.0),
        end: DVec3::new(0.45, 0.026, 0.0),
        up: DVec3::Y,
        profile: Profile::Round {
            start: 0.05,
            end: 0.05,
        },
        material: wood(),
        motion: Motion::default(),
    };
    let built = assemble(&[p], &[]).expect("builds");
    let mut b = Body::structure(built.structure, DT);
    b.shapes = built.shapes;
    let (b, asleep) = settle(b);
    assert!(asleep, "never slept");
    rests_on_the_ground(&b, "pole");
}

/// Boxes centred on a beam's nodes and off them (their arms half a box long): the faces'
/// corners press at lever arms on nodes whose turning inertia is a beam end's.
#[test]
fn a_beam_with_boxes_on_its_nodes_comes_to_rest_on_the_ground() {
    for inwards in [0.0, 0.2] {
        let (b, asleep) = settle(beam_with_boxes(inwards));
        assert!(asleep, "boxes {inwards} m inwards never slept");
        rests_on_the_ground(&b, &format!("boxes {inwards} m inwards"));
    }
}

/// The control: a rigid body with the same box off its centre.
#[test]
fn a_rigid_body_with_an_off_centre_box_comes_to_rest_on_the_ground() {
    let s = StructureBuilder::rigid_body(
        DVec3::new(0.0, 0.026, 0.0),
        1.0,
        DMat3::from_diagonal(DVec3::splat(0.01)),
    );
    let mut b = Body::structure(s, DT);
    let half = DVec3::new(0.2, 0.025, 0.025);
    b.shapes = vec![(0, hull(&box_points(half, DVec3::X * 0.2)))];
    let (b, asleep) = settle(b);
    assert!(asleep, "never slept");
    rests_on_the_ground(&b, "rigid");
}

/// The server's way (G2.3): two poles as bodies crossed on the ground, then merged in place
/// into one body with a joint, stepped on.
#[test]
fn poles_merged_in_a_world_step_on() {
    use creeer_physics::structure::{ImplicitSolver, Scheme};
    let pole = |start: DVec3, end: DVec3| Part {
        start,
        end,
        up: DVec3::Y,
        profile: Profile::Round {
            start: 0.05,
            end: 0.05,
        },
        material: wood(),
        motion: Motion::default(),
    };
    let body = |p: Part| {
        let built = assemble(&[p], &[]).expect("builds");
        let mut b = Body::structure(built.structure, DT);
        b.shapes = built.shapes;
        b
    };
    let mut w = World::new(DT);
    w.statics.push(StaticShape::Plane {
        normal: DVec3::Y,
        offset: 0.0,
    });
    w.add(body(pole(
        DVec3::new(-0.8, 0.025, 0.0),
        DVec3::new(0.8, 0.025, 0.0),
    )));
    w.add(body(pole(
        DVec3::new(0.1, 0.075, -0.8),
        DVec3::new(0.1, 0.075, 0.8),
    )));
    for _ in 0..60 {
        w.step();
    }
    let theirs = w.bodies[1].clone();
    let _ = w.retain_bodies(|k, _| k != 1);
    let b = &mut w.bodies[0];
    let first = b.structure.element_count();
    let offset = b.structure.append(&theirs.structure);
    b.shapes
        .extend(theirs.shapes.iter().map(|(n, s)| (n + offset, s.clone())));
    let na = b.structure.insert_node(0, 0.5625);
    let nb = b.structure.insert_node(first, 0.5);
    let law = JointLaw {
        axial: 1.5e7,
        bearing: 4500.0,
        withdrawal: 700.0,
        lateral: 9.6e5,
        lateral_yield: 1000.0,
        lateral_peak: 1160.0,
        slip_at_peak: 0.01,
        slip_at_failure: 0.02,
        twist: 15.0,
        twist_yield: 4.0,
        interaction: 1.0,
        damping: 0.01,
    };
    b.structure.join(na, nb, &law);
    b.solver = ImplicitSolver::new(&b.structure, Scheme::Newmark, DT, 1);
    b.wake();
    for _ in 0..120 {
        w.step();
    }
    let b = &w.bodies[0];
    assert!(b.structure.position.iter().all(|p| p.is_finite()));
    rests_on_the_ground(b, "merged poles");
}
