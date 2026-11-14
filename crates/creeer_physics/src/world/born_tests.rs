//! A body put into the world pressed into another (G2.11, `Body::born`): what its contacts hold
//! when its first step finds them never existed. The step pushes the two apart as it would any
//! overlap, and none of that energy becomes motion. *log_bridge*'s fallen builder had his head
//! laid out 13.4 cm inside the 200 kg crate that knocked him down: 222 MJ in one contact, and the
//! crate left at 44 m/s, his limbs at up to 115 m/s, the bridge smashed (2.6 s in one tick).
//!
//! The bound: the bodies' energy (motion, gravity's) never above what they had, plus what lifting
//! the pushed-out body by its overlap takes (m·g·depth: the overlap is resolved by moving it);
//! no body faster than falling through the overlap would make it (√(2·g·depth)) beyond its start.

use super::shape::{Shape, StaticShape};
use super::*;
use crate::structure::section::{STEEL_S235, Section};

const DT: f64 = 1.0 / 60.0;
const G: f64 = 9.81;
/// The head's overlap with the crate (m), as measured in *log_bridge*.
const DEPTH: f64 = 0.134;
const HEAD: f64 = 7.25;

/// The ground, a 200 kg crate (1 m) on it (`gap` m above it, falling at `speed` m/s), and a
/// 7.25 kg box (22 cm) put [`DEPTH`] into its top, moving with it: the head added last.
fn crate_and_head(gap: f64, speed: f64) -> World {
    let mut world = World::new(DT);
    world.statics.push(StaticShape::Plane {
        normal: DVec3::Y,
        offset: 0.0,
    });
    let half = DVec3::splat(0.5);
    let mut c = Body::rigid(Shape::Box { half }, 200.0, DVec3::Y * (0.5 + gap), DT);
    c.structure.velocity[0] = DVec3::NEG_Y * speed;
    world.add(c);
    world.step();
    let top = world.bodies[0].structure.position[0].y + 0.5;
    let mut head = Body::rigid(
        Shape::Box {
            half: DVec3::splat(0.11),
        },
        HEAD,
        DVec3::new(0.1, top + 0.11 - DEPTH, 0.0),
        DT,
    );
    head.structure.velocity[0] = world.bodies[0].structure.velocity[0];
    world.add(head);
    world
}

/// The bodies' kinetic and gravity's energy (J), and the fastest body's speed (m/s).
fn energy_and_speed(world: &World) -> (f64, f64) {
    let (mut energy, mut fastest) = (0.0, 0.0f64);
    for b in &world.bodies {
        let (kinetic, potential) = b.structure.node_energy();
        energy += kinetic + potential + b.structure.elastic_energy();
        for v in &b.structure.velocity {
            fastest = fastest.max(v.length());
        }
    }
    (energy, fastest)
}

/// Steps `world` for `ticks`; its most energy and speed over them, beyond the start's.
fn stepped(mut world: World, ticks: usize) -> (World, f64, f64) {
    let (start, speed) = energy_and_speed(&world);
    let (mut gained, mut faster) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for _ in 0..ticks {
        world.step();
        let (e, v) = energy_and_speed(&world);
        gained = gained.max(e - start);
        faster = faster.max(v - speed);
    }
    (world, gained, faster)
}

/// The bound for a box put `depth` into another (see the module docs). What its contacts
/// still hold after its first step is the motion's (`Body::born`; a limit): its first step
/// resolves 79 % of a 13.4 cm overlap, and the rest lifts the box 6.8 mm over where it comes
/// to rest (0.47 J, 5 % of the lift). The gate allows a tenth.
fn assert_bounded(what: &str, depth: f64, (gained, faster): (f64, f64)) {
    let lift = HEAD * G * depth;
    let fall = (2.0 * G * depth).sqrt();
    assert!(
        gained <= 1.1 * lift,
        "{what}: gained {gained:.3} J (lift {lift:.3} J)"
    );
    assert!(
        faster <= fall,
        "{what}: {faster:.3} m/s faster (falling it: {fall:.3})"
    );
}

#[test]
fn a_box_put_into_a_resting_crate_is_pushed_out_not_thrown() {
    let (world, gained, faster) = stepped(crate_and_head(0.0, 0.0), 120);
    assert_bounded("at rest", DEPTH, (gained, faster));
    // Pushed out: it lies on the crate (within the contact's own give), not in it.
    let (c, h) = (&world.bodies[0].structure, &world.bodies[1].structure);
    let into = (c.position[0].y + 0.5) - (h.position[0].y - 0.11);
    assert!(into < 1e-3, "still {into:.4} m in the crate");
}

#[test]
fn a_box_put_into_a_crate_as_it_lands_is_pushed_out_not_thrown() {
    // The crate 2 cm over the ground at 4 m/s: its first step lands it (an impact).
    let (_, gained, faster) = stepped(crate_and_head(0.02, 4.0), 120);
    assert_bounded("landing", DEPTH, (gained, faster));
}

#[test]
fn a_box_put_into_a_struck_beam_is_pushed_out_not_thrown() {
    // A steel beam lying on the ground, the box put 3 cm into its end's joint (a ball 15 cm
    // across: a box put deeper holds the ball's centre, and has no side to be pushed out by),
    // as a 50 kg block lands on its other end at 4 m/s: the group's step is in impact substeps.
    let depth = 0.03;
    let mut world = World::new(DT);
    world.statics.push(StaticShape::Plane {
        normal: DVec3::Y,
        offset: 0.0,
    });
    let section = Section::rectangle(0.15, 0.15);
    let mut b = crate::structure::StructureBuilder::new();
    let (n0, n1) = (
        b.node(DVec3::new(-1.0, 0.075, 0.0)),
        b.node(DVec3::new(1.0, 0.075, 0.0)),
    );
    b.beam(n0, n1, section, STEEL_S235);
    world.add(Body::structure(b.build(), DT));
    world.step();
    let head = Body::rigid(
        Shape::Box {
            half: DVec3::splat(0.11),
        },
        HEAD,
        DVec3::new(-1.0, 0.15 + 0.11 - depth, 0.0),
        DT,
    );
    world.add(head);
    let mut block = Body::rigid(
        Shape::Box {
            half: DVec3::splat(0.1),
        },
        50.0,
        DVec3::new(1.0, 0.15 + 0.1 + 0.01, 0.0),
        DT,
    );
    block.structure.velocity[0] = DVec3::NEG_Y * 4.0;
    world.add(block);
    // (The block's blow tips the beam and tosses the box at 2.2 m/s: no speed bound here.)
    let (_, gained, _) = stepped(world, 120);
    assert_bounded("struck beam", depth, (gained, 0.0));
}
