//! Piles and structures on contacts: a heavy impact into stacks keeps its energy in check,
//! and structures resting on contacts converge quickly.

use super::shape::{Shape, StaticShape};
use super::*;
use crate::structure::Section;
use crate::structure::section::STEEL_S235;
use crate::structure::shapes::{Base, space_frame};

const DT: f64 = 1.0 / 60.0;

fn ground() -> StaticShape {
    StaticShape::Plane {
        normal: DVec3::Y,
        offset: 0.0,
    }
}

/// An 800 kg ball at 11.5 to 14 m/s smashes into five stacks of six 80 kg crates. The crates
/// tumble, collide and pile up; the total energy (kinetic, gravity, contacts) may only go
/// down, tick by tick: no contact may throw a crate with energy that never existed.
/// Six speeds, each a different chaos: one trajectory let two faults pass for a long time
/// (a crate spun up within a step and buried a corner in its neighbour; a lifted patch
/// corner's dashpot went NaN), both found at 12.5 m/s in F5. Within 10⁻⁴ of the start: at
/// 12.75 and 14 m/s contacts frozen for the step let the ball end a step inside a crate's
/// edge and two crates' crossed edges in each other, 113 J (`contact/turning.rs`; 4.5 J the
/// most now, over seventeen speeds from 11 to 15 m/s).
#[test]
fn a_ball_smashing_into_stacks_never_gains_energy() {
    for speed in [11.5, 12.5, 12.75, 14.0, 14.25, 14.5] {
        smash(speed);
    }
}

fn smash(speed: f64) {
    let mut world = World::new(DT);
    world.statics.push(ground());
    for stack in 0..5 {
        for level in 0..6 {
            let at = DVec3::new(f64::from(stack) * 1.4 - 2.8, 0.5 + f64::from(level), 0.0);
            let half = DVec3::splat(0.5);
            let mut crate_ = Body::rigid(Shape::Box { half }, 80.0, at, DT);
            crate_.friction = 0.6;
            world.add(crate_);
        }
    }
    let at = DVec3::new(-2.8, 3.5, -12.0);
    let mut ball = Body::rigid(Shape::Sphere { radius: 0.6 }, 800.0, at, DT);
    ball.structure.velocity[0] = DVec3::new(0.0, 2.0, speed);
    ball.restitution = 0.3;
    world.add(ball);
    let start = world.energy();
    let mut energy = start;
    for tick in 0..180 {
        world.step();
        let now = world.energy();
        assert!(
            now <= energy + 1e-4 * start,
            "{speed} m/s, tick {tick}: energy rose {energy:.0} -> {now:.0} J"
        );
        energy = now;
    }
    // The impact did something: the hit stack is down.
    let top = world.bodies[5].structure.position[0].y;
    assert!(
        top < 4.0,
        "{speed} m/s: the hit stack still stands ({top} m)"
    );
}

/// A 40 m steel bridge resting on two piers (by contact, with friction) under a 10 t
/// load: its Newton matrix includes the contacts' stiffness (without it: 48 iterations a
/// step on the piers alone, 72 with the load), and the load is solved jointly with it
/// (`structure/coupled.rs`; without: 15.5), so a step takes about two: one and the check.
#[test]
fn a_bridge_resting_on_contacts_converges_quickly() {
    let mut world = World::new(DT);
    world.statics.push(ground());
    let tube = Section::round_tube(0.16, 0.008);
    let mut deck = space_frame([20, 1, 2], 2.0, tube, STEEL_S235, Base::Free);
    deck.translate(DVec3::new(-20.0, 4.0 + tube.extent[0], -2.0));
    world.add(Body::structure(deck, DT));
    for x in [-20.5, 20.5] {
        world.statics.push(StaticShape::Box {
            center: DVec3::new(x, 2.0, 0.0),
            rotation: DQuat::IDENTITY,
            half: DVec3::new(1.0, 2.0, 3.0),
        });
    }
    let half = DVec3::new(2.2, 0.5, 2.2);
    let top = 6.0 + 2.0 * tube.extent[0];
    let load = Body::rigid(Shape::Box { half }, 10_000.0, DVec3::Y * (top + 0.5), DT);
    world.add(load);
    let mut iterations = 0;
    // At rest: after the block's landing (members yield in impact substeps) and the rocking
    // on the swinging deck have died away. When that happens depends on the exact path: a
    // micrometre on where the block starts moved it between the first and the sixth second
    // (F5).
    for tick in 0..600 {
        world.step();
        if tick >= 480 {
            iterations += world.bodies[0].solver.last_iterations;
        }
    }
    let mean = f64::from(iterations) / 120.0;
    assert!(mean < 3.0, "{mean} iterations per step");
}
