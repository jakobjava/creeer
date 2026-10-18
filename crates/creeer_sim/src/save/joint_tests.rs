//! Version 8 (G1.1): the physics world keeps its joints, their constraint forces and their
//! friction's anchors, so a loaded world goes on as the saved one would have.

use super::tests::scene;
use super::*;
use creeer_math::glam::DVec3;
use creeer_physics::world::Body;
use creeer_physics::world::joint::{Joint, Kind};
use creeer_physics::world::shape::Shape;

const V8: &[u8] = include_bytes!("fixtures/v8_g1_world.bin");
const DT: f64 = 1.0 / 60.0;

/// The fixtures' scene with a two-link pendulum beside it: a hinge to the world with stops and
/// friction, a ball joint with damping, a third of a second after it was let go.
fn joint_scene() -> Save {
    let mut save = scene();
    let world = &mut save.world;
    let ball = |at: DVec3| Body::rigid(Shape::Sphere { radius: 0.1 }, 2.0, at, DT);
    let a = world.add(ball(DVec3::new(5.0, 3.0, 0.0)));
    let b = world.add(ball(DVec3::new(5.5, 3.0, 0.0)));
    let hinge = Joint::new(
        world,
        ((a, 0), None),
        (DVec3::new(4.5, 3.0, 0.0), DVec3::Z, DVec3::X),
        Kind::Hinge {
            range: Some((-0.4, 1.2)),
        },
    );
    let h = world.join(hinge);
    world.joint_mut(h).friction = 0.5;
    let link = Joint::new(
        world,
        ((b, 0), Some((a, 0))),
        (DVec3::new(5.25, 3.0, 0.0), DVec3::X, DVec3::Y),
        Kind::Ball {
            cone: Some(1.0),
            twist: None,
        },
    );
    let l = world.join(link);
    world.joint_mut(l).damping = 0.3;
    for _ in 0..20 {
        world.step();
    }
    save.entities.extend([(9, None), (10, None)]);
    save.next_id = 11;
    save
}

#[test]
#[ignore = "writes the version 8 fixture"]
fn write_v8_fixture() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/save/fixtures/v8_g1_world.bin"
    );
    std::fs::write(path, joint_scene().to_bytes()).unwrap();
}

#[test]
fn the_version_8_fixture_keeps_its_joints() {
    let save = Save::from_bytes(V8).unwrap();
    assert_eq!(save.to_bytes(), V8);
    let written = joint_scene();
    assert_eq!(save.to_bytes(), written.to_bytes());
    let joints = save.world.joints();
    assert_eq!(joints.len(), 2);
    assert!(joints[0].anchor.is_some(), "the hinge's friction anchor");
    assert!(
        joints[1].multipliers.point.length() > 0.0,
        "the link's force"
    );
    // It goes on as the saved world would have, bit for bit.
    let (mut loaded, mut world) = (save.world, written.world);
    for _ in 0..30 {
        loaded.step();
        world.step();
    }
    assert_eq!(loaded.to_bytes(), world.to_bytes());
}
