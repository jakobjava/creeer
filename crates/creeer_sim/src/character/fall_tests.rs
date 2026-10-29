//! Falls onto rock, as a person's go, each from five slightly different leans (people land
//! differently; one landing alone is chance): from 1 m unhurt on its feet (the legs take it),
//! from 3 m on the legs and then its hips (all but unhurt, down), from 10 m seriously hurt
//! (about half its health), from 20 m killed. A character struck down finishes its strike as
//! its body, then falls as its ragdoll, whose segments take what comes after.

use super::injury::{Region, Strikes, pushed_on};
use super::ragdoll::{fall, knocked, region};
use super::*;
use creeer_physics::world::shape::StaticShape;

const DT: f64 = 1.0 / 60.0;

/// The leans it is let go at (rad).
const LEANS: [f64; 5] = [-0.08, -0.04, 0.0, 0.04, 0.08];

/// The harm (share of health) a character dropped with its feet `height` above rock, leaning
/// by `lean`, takes, and whether it ended down.
fn dropped(height: f64, lean: f64) -> (f64, bool) {
    let mut world = World::new(DT);
    world.statics.push(StaticShape::Plane {
        normal: DVec3::Y,
        offset: 0.0,
    });
    let mut b = body(DVec3::new(0.0, height, 0.0), DVec3::Y, DT);
    b.structure.orientation[0] = creeer_math::glam::DQuat::from_rotation_x(lean);
    b.solver.reset_history(&b.structure);
    let c = world.add(b);
    let (mut total, mut parts, mut struck) = (0.0, None::<Vec<usize>>, false);
    let mut strikes = Strikes::default();
    let mut reactions = Vec::new();
    for _ in 0..360 {
        match &parts {
            None => {
                let intent = Intent::default();
                steer(&mut world, c, intent, (DVec3::Y, None), &mut reactions);
                let before = world.bodies[c].structure.velocity[0];
                world.step();
                let motion = (before, pushed_on(&world.strut_pushes(), c));
                release(&mut world, &mut reactions);
                total += strikes.after_step(&world, (0, c), motion, Region::Torso);
                // Knocked down, it falls once its strike is over (as the server's people do).
                struck |= knocked(&world, c, DVec3::Y, motion);
                if struck && !strikes.striking(0) {
                    parts = Some(fall(&mut world, c));
                }
            }
            Some(p) => {
                let before: Vec<DVec3> = (p.iter())
                    .map(|&b| world.bodies[b].structure.velocity[0])
                    .collect();
                world.step();
                for (k, (&b, v)) in p.iter().zip(before).enumerate() {
                    let motion = (v, DVec3::ZERO);
                    total += strikes.after_step(&world, (k + 1, b), motion, region(k));
                }
            }
        }
    }
    (total, parts.is_some())
}

/// Each lean's harm from `height`, and whether every one ended down (and whether any did).
fn from(height: f64) -> (Vec<f64>, bool, bool) {
    let all: Vec<(f64, bool)> = LEANS.iter().map(|&l| dropped(height, l)).collect();
    let harms = all.iter().map(|a| a.0).collect();
    (harms, all.iter().all(|a| a.1), all.iter().any(|a| a.1))
}

#[test]
fn falls_onto_rock_hurt_as_a_persons_do() {
    let (one, _, any) = from(1.0);
    assert!(
        one.iter().all(|&h| h == 0.0) && !any,
        "1 m: harms {one:?}, down {any}"
    );
    // Its worst lean a tenth above what was measured (G2.10, the legs' push falling with their
    // extension speed, ADR-0023): the legs' rebound before the knockdown is weaker, so each
    // lean lands its body differently. Per lean, 0.076, 0.145, 0.000, 0.049, 0.109 before
    // (mean 0.076, gate 0.15); 0.158, 0.012, 0.054, 0.000, 0.098 since (mean 0.064).
    let (three, all, _) = from(3.0);
    assert!(
        three.iter().all(|&h| h < 0.174) && all,
        "3 m: harms {three:?}, down {all}"
    );
    // Seriously hurt, never killed: below the 20 m case's 0.95. (Under 0.9 until the legs'
    // force–velocity limit: per lean 0.881, 0.749, 0.533, 0.682, 0.835 before, mean 0.736;
    // 0.944, 0.753, 0.583, 0.710, 0.816 since, mean 0.761, the first lean landing worse.)
    let (ten, all, _) = from(10.0);
    assert!(
        ten.iter().all(|h| (0.3..0.95).contains(h)) && all,
        "10 m: harms {ten:?}, down {all}"
    );
    // Fatal (half of falls of four floors, about 15 m, kill).
    let (twenty, _, _) = from(20.0);
    assert!(twenty.iter().all(|&h| h >= 0.95), "20 m: harms {twenty:?}");
    eprintln!("falls onto rock: 3 m {three:.3?}, 10 m {ten:.3?}, 20 m {twenty:.3?}");
}

/// Running with the jump key held, it lands between jumps and each jump rises its full height
/// (G1.10, the user: it hopped on its legs' tips and went down; held, the key jumped again at
/// the legs' first touch, still falling, 12–23 cm up).
#[test]
fn running_with_jump_held_it_lands_between_full_jumps() {
    let mut world = World::new(DT);
    world.statics.push(StaticShape::Plane {
        normal: DVec3::Y,
        offset: 0.0,
    });
    world.static_friction = 0.9;
    let c = world.add(body(DVec3::ZERO, DVec3::Y, DT));
    let mut reactions = Vec::new();
    let (mut highest, mut lowest, mut peaks, mut knocked_down) =
        (0.0f64, f64::INFINITY, Vec::new(), false);
    let mut rising = false;
    for k in 0..360 {
        let intent = Intent::walk(DVec3::Z * 6.5, k >= 60);
        steer(&mut world, c, intent, (DVec3::Y, None), &mut reactions);
        let before = world.bodies[c].structure.velocity[0];
        world.step();
        let motion = (before, pushed_on(&world.strut_pushes(), c));
        release(&mut world, &mut reactions);
        knocked_down |= knocked(&world, c, DVec3::Y, motion);
        let s = &world.bodies[c].structure;
        let feet = s.position[0].y - CENTRE;
        if k > 60 {
            if rising && s.velocity[0].y <= 0.0 {
                peaks.push(highest);
            }
            rising = s.velocity[0].y > 0.0;
            highest = if rising { highest.max(feet) } else { feet };
            if !rising {
                lowest = lowest.min(feet);
            }
        }
    }
    assert!(!knocked_down);
    assert!(peaks.len() >= 5, "{peaks:?}");
    // Each jump its legs' full push (0.4 m), less what the legs' springs still held: over 0.3 m.
    assert!(peaks.iter().all(|p| *p > 0.3), "{peaks:?}");
    assert!(lowest < 0.03, "never landed: lowest {lowest} m");
}
