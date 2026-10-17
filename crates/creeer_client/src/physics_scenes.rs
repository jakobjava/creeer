//! The structural scenes of the physics sandbox (`physics_sandbox.rs`): a bridge under a
//! falling load, a vehicle frame crashing into a wall, stacks of crates hit by a ball.

use creeer_math::glam::{DMat3, DQuat, DVec3};
use creeer_physics::structure::Section;
use creeer_physics::structure::section::STEEL_S235;
use creeer_physics::structure::shapes::{Base, space_frame};
use creeer_physics::world::shape::{Shape, StaticShape};
use creeer_physics::world::{Body, World};

pub(crate) fn wall(world: &mut World, center: DVec3, half: DVec3) {
    world.statics.push(StaticShape::Box {
        center,
        rotation: DQuat::IDENTITY,
        half,
    });
}

pub(crate) fn bridge(world: &mut World) {
    let dt = world.dt();
    let tube = Section::round_tube(0.16, 0.008);
    let mut deck = space_frame([20, 1, 2], 2.0, tube, STEEL_S235, Base::Free);
    // Simply supported: the bottom joints (balls of the tube's radius) rest on the piers.
    deck.translate(DVec3::new(-20.0, 4.0 + tube.extent[0], -2.0));
    world.add(Body::structure(deck, dt));
    for x in [-20.5, 20.5] {
        wall(world, DVec3::new(x, 2.0, 0.0), DVec3::new(1.0, 2.0, 3.0));
    }
    // A 4.4 m square load over nine deck joints, dropped from 0.5 m.
    let deck_top = 6.0 + 2.0 * tube.extent[0];
    let block = Body::rigid(
        Shape::Box {
            half: DVec3::new(2.2, 0.5, 2.2),
        },
        10_000.0,
        DVec3::new(0.0, deck_top + 1.0, 0.0),
        dt,
    );
    world.add(block);
}

pub(crate) fn crash(world: &mut World) {
    let dt = world.dt();
    let tube = Section::round_tube(0.08, 0.004);
    let mut car = space_frame([2, 1, 1], 2.0, tube, STEEL_S235, Base::Free);
    // Engine, body and cargo: 1.2 t spread over the joints.
    for node in 0..car.node_count() {
        car.add_mass(node, 100.0, DMat3::ZERO);
    }
    car.translate(DVec3::new(-3.0, tube.extent[0] + 0.005, -1.0));
    for v in &mut car.velocity {
        *v = DVec3::X * 15.0;
    }
    world.add(Body::structure(car, dt));
    wall(world, DVec3::new(3.0, 1.5, 0.0), DVec3::new(0.5, 1.5, 4.0));
}

pub(crate) fn stacks(world: &mut World) {
    let dt = world.dt();
    for stack in 0..5 {
        for level in 0..6 {
            let position = DVec3::new(f64::from(stack) * 1.4 - 2.8, 0.5 + f64::from(level), 0.0);
            let mut crate_ = Body::rigid(
                Shape::Box {
                    half: DVec3::splat(0.5),
                },
                80.0,
                position,
                dt,
            );
            crate_.friction = 0.6;
            world.add(crate_);
        }
    }
    let mut ball = Body::rigid(
        Shape::Sphere { radius: 0.6 },
        800.0,
        DVec3::new(-2.8, 3.5, -12.0),
        dt,
    );
    ball.structure.velocity[0] = DVec3::new(0.0, 2.0, 12.0);
    ball.restitution = 0.3;
    world.add(ball);
}
