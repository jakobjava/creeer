//! Contacts under loads far above the weight of the bodies they join: a tall column of
//! crates, a heavy block on a light crate, a loaded frame standing on its joints. The
//! resolvable stiffness (`contact/mod.rs`) may not let them sag more than physics and
//! the step's resolution allow.

use super::shape::{Shape, StaticShape};
use super::surface::Surface;
use super::*;
use crate::structure::section::STEEL_S235;
use crate::structure::shapes::{Base, space_frame};
use crate::structure::{Section, Structure};

const DT: f64 = 1.0 / 60.0;
const G: f64 = 9.806_65;

fn ground() -> StaticShape {
    StaticShape::Plane {
        normal: DVec3::Y,
        offset: 0.0,
    }
}

fn crate_at(y: f64, half: DVec3, mass: f64) -> Body {
    Body::rigid(Shape::Box { half }, mass, DVec3::Y * y, DT)
}

/// Steps until every body sleeps or `ticks` passed.
fn settle(world: &mut World, ticks: usize) {
    for _ in 0..ticks {
        world.step();
        if world.bodies.iter().all(|b| b.sleep.asleep) {
            break;
        }
    }
}

/// A column of thirty 100 kg crates settles as its contacts' law says: the contact j crates
/// from the top carries j crates, 2·j times the weight of the pair it joins, and sinks the
/// reach d per e-fold of that, d·(1 + ln 2j); the ground under it carries 30 crates on one.
/// 1.50 mm in all (with a cap that did not stiffen under load: 10.9 mm; real steel: µm).
#[test]
fn a_column_of_crates_sinks_a_reach_per_e_fold_of_its_load() {
    let mut world = World::new(DT);
    world.statics.push(ground());
    let n = 30;
    for level in 0..n {
        world.add(crate_at(0.5 + f64::from(level), DVec3::splat(0.5), 100.0));
    }
    settle(&mut world, 600);
    assert!(world.bodies.iter().all(|b| b.sleep.asleep), "settled");
    let top = world.bodies[n as usize - 1].structure.position[0].y;
    let sag = f64::from(n) - 0.5 - top;
    let d = contact::reach(DT);
    let ln = creeer_math::det::ln;
    let want = (1..n)
        .map(|j| d * (1.0 + ln(2.0 * f64::from(j))))
        .sum::<f64>()
        + d * (1.0 + ln(f64::from(n)));
    assert!(
        (sag / want - 1.0).abs() < 0.03,
        "sank {sag:e} m, expected {want:e}"
    );
}

/// A 10 t block on a 100 kg crate: both contacts carry a hundred times the weight of the
/// pair they join, and sink d·(1 + ln 100) each, 0.13 mm in all (2.45 mm before).
#[test]
fn a_heavy_block_on_a_light_crate_sinks_micrometres() {
    let mut world = World::new(DT);
    world.statics.push(ground());
    world.add(crate_at(0.5, DVec3::splat(0.5), 100.0));
    world.add(crate_at(1.5, DVec3::splat(0.5), 10_000.0));
    settle(&mut world, 600);
    let sag = 1.5 - world.bodies[1].structure.position[0].y;
    assert!(sag < 0.2e-3, "sank {sag:e} m");
}

/// A steel frame carrying 100 t stands on its four joints on concrete: each sinks exactly
/// Hertz's depth (F/k)^⅔ of a sphere its joint's size, 0.78 mm, the step resolving its
/// stiffness at that load. (A cap at the joint's own mass made it 1.21 mm.)
#[test]
fn a_loaded_frame_stands_on_its_joints_at_hertz_depth() {
    let tube = Section::round_tube(0.2, 0.02);
    let mut frame: Structure = space_frame([1, 1, 1], 2.0, tube, STEEL_S235, Base::Free);
    let radius = tube.extent[0].max(tube.extent[1]);
    frame.translate(DVec3::Y * radius);
    let load = 100_000.0 * G;
    let n = frame.node_count();
    let top: Vec<usize> = (0..n).filter(|&i| frame.position[i].y > 1.0).collect();
    let feet: Vec<usize> = (0..n).filter(|&i| frame.position[i].y < 1.0).collect();
    #[allow(clippy::cast_precision_loss)] // four
    let share = load / top.len() as f64;
    for &i in &top {
        frame.load[i] = DVec3::NEG_Y * share;
    }
    let mut world = World::new(DT);
    world.statics.push(ground());
    world.add(Body::structure(frame, DT));
    settle(&mut world, 600);
    assert!(world.bodies[0].sleep.asleep, "settled");
    let s = &world.bodies[0].structure;
    #[allow(clippy::cast_precision_loss)] // four
    let per_foot = (s.masses().iter().sum::<f64>() * G + load) / feet.len() as f64;
    let modulus = Surface::STEEL.contact_modulus(&world.static_surface);
    let k = 4.0 / 3.0 * modulus * radius.sqrt();
    let root = creeer_math::det::cbrt(per_foot / k);
    let hertz = root * root;
    for &i in &feet {
        let sunk = radius - s.position[i].y;
        assert!(
            (sunk / hertz - 1.0).abs() < 0.01,
            "sank {sunk:e} m, Hertz {hertz:e} m"
        );
    }
}
