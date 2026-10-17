//! Bodies are drawn as what they are, and a structure's shape is told again only where it
//! changed by more than the tolerance, and not more often than every few ticks.

use super::*;
use creeer_math::glam::DQuat;
use creeer_physics::structure::Section;
use creeer_physics::structure::section::STEEL_S235;
use creeer_physics::structure::shapes::{Base, space_frame};
use creeer_physics::world::hull::Hull;
use std::sync::Arc;

const DT: f64 = 1.0 / 60.0;

fn frame() -> Body {
    let s = space_frame(
        [1, 1, 1],
        1.0,
        Section::round_tube(0.05, 0.003),
        STEEL_S235,
        Base::Free,
    );
    Body::structure(s, DT)
}

#[test]
fn bodies_are_drawn_as_what_they_are() {
    let crate_ = Body::rigid(
        Shape::Box {
            half: DVec3::new(0.5, 0.25, 1.0),
        },
        10.0,
        DVec3::ZERO,
        DT,
    );
    assert_eq!(
        kind(&crate_, None),
        Kind::Body(NetShape::Box(DVec3::new(0.5, 0.25, 1.0)))
    );
    let ball = Body::rigid(Shape::Sphere { radius: 0.4 }, 10.0, DVec3::ZERO, DT);
    assert_eq!(kind(&ball, None), Kind::Body(NetShape::Sphere(0.4)));
    assert_eq!(
        kind(&ball, Some("ada")),
        Kind::Avatar {
            radius: 0.4,
            player: "ada".into()
        }
    );
    let points = [DVec3::ZERO, DVec3::X, DVec3::Y, DVec3::Z, DVec3::ONE];
    let (hull, _) = Hull::new(&points).unwrap();
    let corners = hull.vertices().to_vec();
    let rock = Body::rigid(Shape::Hull(Arc::new(hull)), 10.0, DVec3::ZERO, DT);
    assert_eq!(kind(&rock, None), Kind::Body(NetShape::Hull(corners)));
    let Kind::Structure(f) = kind(&frame(), None) else {
        panic!("a structure")
    };
    assert_eq!((f.nodes.len(), f.members.len()), (8, 12));
    assert!((f.nodes[7] - DVec3::ONE).length() < 1e-3);
    assert!((f.radius - 0.025).abs() < 1e-12);
}

#[test]
fn a_structure_is_drawn_in_its_own_frame() {
    let mut body = frame();
    let turn = DQuat::from_rotation_y(0.7);
    let s = &mut body.structure;
    for (p, q) in s.position.iter_mut().zip(&mut s.orientation) {
        *p = DVec3::new(100.0, 5.0, -3.0) + turn * *p;
        *q = turn;
    }
    let Kind::Structure(f) = kind(&body, None) else {
        panic!("a structure")
    };
    // Relative to node 0, in its frame: as built.
    assert!((f.nodes[7] - DVec3::ONE).length() < 1e-3, "{}", f.nodes[7]);
    let state = state(&body);
    assert!((state.position() - DVec3::new(100.0, 5.0, -3.0)).length() < 1e-3);
}

#[test]
fn shape_changes_are_told_past_the_tolerance_and_not_too_often() {
    let mut body = frame();
    let mut shapes = Shapes::default();
    let id = EntityId(4);
    shapes.kind(id, &body, None, 0);
    let changes =
        |shapes: &mut Shapes, body: &Body, tick| shapes.changes(std::iter::once((id, body)), tick);
    // A sway of 5 mm: nothing.
    body.structure.position[3] += DVec3::new(0.005, 0.0, 0.0);
    assert!(changes(&mut shapes, &body, 10).is_empty());
    // Bent by 3 cm at node 3: that node, once.
    body.structure.position[3] += DVec3::new(0.025, 0.0, 0.0);
    let told = changes(&mut shapes, &body, 20);
    assert_eq!(told.len(), 1);
    assert_eq!(
        told[0].1.nodes.iter().map(|n| n.0).collect::<Vec<_>>(),
        vec![3]
    );
    assert!(told[0].1.members.is_none() && told[0].1.broken.is_empty());
    // Bent again within a few ticks of being told: told once they have passed.
    body.structure.position[5] += DVec3::new(0.0, 0.05, 0.0);
    assert!(changes(&mut shapes, &body, 20 + SHAPE_EVERY - 1).is_empty());
    assert_eq!(changes(&mut shapes, &body, 20 + SHAPE_EVERY).len(), 1);
    assert!(changes(&mut shapes, &body, 40).is_empty());
    // Gone from the world: forgotten.
    assert!(shapes.changes(std::iter::empty(), 100).is_empty());
    assert!(shapes.told.is_empty());
}
