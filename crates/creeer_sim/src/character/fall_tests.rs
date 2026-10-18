//! Falls onto rock, as a person's go: from 1 m unhurt (the legs take it), from 3 m on the
//! legs and then its hips (unhurt, but down), from 10 m seriously hurt (half its health), from
//! 20 m killed. A character knocked down falls as its ragdoll, whose segments take the rest of
//! the impacts.

use super::injury::{HEAD_STIFFNESS, Strikes, TORSO_STIFFNESS};
use super::ragdoll::{HEAD, fall, knocked};
use super::*;
use creeer_physics::world::shape::StaticShape;

const DT: f64 = 1.0 / 60.0;

/// The harm (share of health) a character dropped with its feet `height` above rock takes,
/// and whether it ended down.
fn dropped(height: f64) -> (f64, bool) {
    let mut world = World::new(DT);
    world.statics.push(StaticShape::Plane {
        normal: DVec3::Y,
        offset: 0.0,
    });
    let c = world.add(body(DVec3::new(0.0, height, 0.0), DVec3::Y, DT));
    let (mut total, mut parts) = (0.0, None::<Vec<usize>>);
    let mut strikes = Strikes::default();
    let mut reactions = Vec::new();
    for _ in 0..360 {
        match &parts {
            None => {
                steer(
                    &mut world,
                    c,
                    Intent::default(),
                    (DVec3::Y, None),
                    &mut reactions,
                );
                let before = world.bodies[c].structure.velocity[0];
                world.step();
                release(&mut world, &mut reactions);
                total += strikes.after_step(&world, (0, c), before, (TORSO_STIFFNESS, false));
                if knocked(&world, c, DVec3::Y, before) {
                    total += strikes.end(&world, (0, c), (TORSO_STIFFNESS, false));
                    parts = Some(fall(&mut world, c));
                }
            }
            Some(p) => {
                let before: Vec<DVec3> = (p.iter())
                    .map(|&b| world.bodies[b].structure.velocity[0])
                    .collect();
                world.step();
                for (k, (&b, v)) in p.iter().zip(before).enumerate() {
                    let head = k == HEAD;
                    let stiffness = if head {
                        HEAD_STIFFNESS
                    } else {
                        TORSO_STIFFNESS
                    };
                    total += strikes.after_step(&world, (k + 1, b), v, (stiffness, head));
                }
            }
        }
    }
    (total, parts.is_some())
}

#[test]
fn falls_onto_rock_hurt_as_a_persons_do() {
    let (one, down) = dropped(1.0);
    assert!(one == 0.0 && !down, "1 m: harm {one}, down {down}");
    let (three, down) = dropped(3.0);
    assert!(three < 0.1 && down, "3 m: harm {three}, down {down}");
    let (ten, down) = dropped(10.0);
    assert!(
        (0.15..1.0).contains(&ten) && down,
        "10 m: harm {ten}, down {down}"
    );
    let (twenty, _) = dropped(20.0);
    eprintln!("falls onto rock: 1 m {one:.3}, 3 m {three:.3}, 10 m {ten:.3}, 20 m {twenty:.3}");
    // Fatal, or as near as makes no difference (half of falls of four floors kill).
    assert!(twenty >= 0.95, "20 m: harm {twenty}");
}
