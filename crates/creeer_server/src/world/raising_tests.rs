//! Raising a long thing by one end (G2.9, the user's play: "how to dig it into the ground?"):
//! a pole grabbed at its end is held by that point, its other end on the ground; walked under,
//! hand over hand, its foot set in a hole, it comes up to stand; a crate is still held level.

use super::tests::world;
use super::*;
use creeer_math::glam::DQuat;
use creeer_net::message::Target;
use creeer_sim::items::{Form, Item, Piece};
use creeer_terrain::edits::{Brush, Edit};

pub(super) fn wait(w: &mut ServerWorld, ada: EntityId, n: u32, walk: DVec3) {
    let input = AvatarInput {
        walk,
        ..AvatarInput::default()
    };
    for _ in 0..n {
        w.steer(ada, &input);
        w.step();
    }
}

/// A world with ada standing, a hole dug 3.6 m east of her (15 cm across in radius, 0.7 m
/// deep), a 3 m green spruce pole lying east–west with its foot 20 cm down the hole, ada at its top
/// end: the world, ada, the pole, the way east, and the hole's mouth.
pub(super) fn ada_and_pole() -> (ServerWorld, EntityId, EntityId, DVec3, DVec3) {
    let mut w = world();
    let ada = w.avatar_of("ada");
    wait(&mut w, ada, 60, DVec3::ZERO);
    let centre = w.world.bodies[w.index(ada).unwrap()].structure.position[0];
    let up = character::plumb(&w.world, centre);
    let (east, _) = creeer_terrain::geology::deposits::tangent_frame(up);
    let mouth = w.ground_under(centre + east * 3.6);
    let down = -mouth.normalize();
    w.terrain.edit(Edit::Dig(Brush::Capsule {
        a: mouth,
        b: mouth + down * 0.7,
        radius: 0.15,
    }));
    let pole = Item::Piece(
        Piece::solid(
            "spruce_green",
            800.0,
            Form::Rod {
                radius: 0.06,
                length: 3.0,
            },
        ),
        Vec::new(),
    );
    // Its foot set 20 cm down the hole first (as a post is raised: the hole's wall braces it),
    // the rest of it lying west on the ground.
    let foot = mouth - mouth.normalize() * 0.2;
    let lying = w.ground_under(mouth - east * 3.0) + up * 0.06;
    let top = foot + (lying - foot).normalize() * 3.0;
    let middle = 0.5 * (foot + top);
    let id = w.add_item(pole, middle, false);
    let i = w.index(id).unwrap();
    let body = &mut w.world.bodies[i];
    // A rod lies along its x: its foot (+x) in the hole, its top (−x) west.
    body.structure.orientation[0] = DQuat::from_rotation_arc(DVec3::X, (foot - top).normalize());
    body.structure.position[0] = middle;
    body.solver.reset_history(&body.structure);
    wait(&mut w, ada, 60, DVec3::ZERO);
    (w, ada, id, east, mouth)
}

/// Grabbed at its west end, a pole is held by that point, not level.
fn grab_the_top(w: &mut ServerWorld, ada: EntityId, id: EntityId, east: DVec3) {
    let s = &w.world.bodies[w.index(id).unwrap()].structure;
    // On its top surface, near its west end.
    let top = s.position[0] - s.orientation[0] * DVec3::X * 1.4 + s.position[0].normalize() * 0.06;
    let _ = east;
    #[allow(clippy::cast_precision_loss)]
    let seen = w.tick as f64;
    let target = Target {
        body: Some(id),
        at: top,
        seen,
    };
    w.apply(&Command::Grab(target), Some(ada)).unwrap();
}

/// The pole's axis's angle from the vertical (degrees).
fn tilt(w: &ServerWorld, id: EntityId) -> f64 {
    let s = &w.world.bodies[w.index(id).unwrap()].structure;
    let axis = s.orientation[0] * DVec3::X;
    let up = s.position[0].normalize();
    creeer_math::det::acos(axis.dot(up).abs()).to_degrees()
}

#[test]
fn a_pole_grabbed_at_its_end_is_raised_by_that_end() {
    let (mut w, ada, id, east, _) = ada_and_pole();
    grab_the_top(&mut w, ada, id, east);
    let c = w.carrying.get(&ada).expect("held");
    // 27 kg levered 1.5 m about the hands: both of them, and their 57.5 N·m cannot hold it
    // level, so its far end stays down (G2.10: no mode of its own, the grip's torque).
    assert!(!c.one_hand, "held in both hands");
    wait(&mut w, ada, 120, DVec3::ZERO);
    assert_eq!(w.carried(ada), Some(id), "{:?}", w.take_notes());
    // Its top up at about head height, its foot still down: tilted, not level.
    let t = tilt(&w, id);
    assert!(t < 80.0 && t > 20.0, "{t:.0}° from the vertical");
}

#[test]
fn walked_up_its_foot_in_a_hole_a_pole_comes_up_to_stand() {
    let (mut w, ada, id, east, mouth) = ada_and_pole();
    grab_the_top(&mut w, ada, id, east);
    // Raised straight up first, then walked under towards its foot, slowly, the hands pushed
    // up to head height before the body and the pole turned towards upright, as a player
    // steers it (G2.10; brought in to the elbows by themselves, the hands drew its foot out).
    let mut most_upright = 90.0f64;
    for round in 0..15 {
        for _ in 0..30 {
            let s = &w.world.bodies[w.index(id).unwrap()].structure;
            let (axis, q) = (s.orientation[0] * DVec3::X, s.orientation[0]);
            let centre = w.world.bodies[w.index(ada).unwrap()].structure.position[0];
            let up = centre.normalize();
            // Its top (the end held, its −x) up: the axis turned towards −up; the hands at head
            // height before the body (the hold steers the pole's centre: its grip turned off it).
            let turn = DQuat::from_rotation_arc(axis, -up) * q;
            let grip = w.carrying.get(&ada).map_or(DVec3::ZERO, |c| c.grip);
            let hold = creeer_net::message::Hold {
                offset: up * 0.75 + east * 0.45 - turn * grip,
                turn,
                steady: false,
            };
            let input = AvatarInput {
                walk: if round < 3 { DVec3::ZERO } else { east * 0.5 },
                hold: Some(hold),
                ..AvatarInput::default()
            };
            w.steer(ada, &input);
            w.step();
            let s = &w.world.bodies[w.index(id).unwrap()].structure;
            if std::env::var("POLE_PROBE").is_ok() {
                eprintln!(
                    "v {:.2} w {:.2} held {}",
                    s.velocity[0].length(),
                    s.angular_velocity[0].length(),
                    w.carried(ada).is_some()
                );
            }
        }
        most_upright = most_upright.min(tilt(&w, id));
        let s = &w.world.bodies[w.index(id).unwrap()].structure;
        let foot = s.position[0] + s.orientation[0] * DVec3::X * 1.5;
        let grip = w.carrying.get(&ada).map(|c| c.grip.x);
        eprintln!(
            "tilt {:.0}°, foot {:.2} m below the mouth, {:.2} m east of it, grip {grip:?}",
            tilt(&w, id),
            (mouth - foot).dot(mouth.normalize()),
            (foot - mouth).dot(east)
        );
        // Standing within 20° of upright, let go (to backfill it): it leans in its hole.
        if tilt(&w, id) < 20.0 {
            w.apply(&Command::Drop, Some(ada)).unwrap();
            wait(&mut w, ada, 120, DVec3::ZERO);
            break;
        }
        if w.carried(ada).is_none() {
            break;
        }
    }
    let s = &w.world.bodies[w.index(id).unwrap()].structure;
    let foot = s.position[0] + s.orientation[0] * DVec3::X * 1.5;
    let below = (mouth - foot).dot(mouth.normalize());
    println!(
        "most upright {most_upright:.0}°, now {:.0}°, its foot {below:.2} m below the mouth",
        tilt(&w, id)
    );
    assert!(most_upright < 30.0, "it came up to {most_upright:.0}° only");
    assert!(
        tilt(&w, id) < 30.0,
        "let go, it fell to {:.0}°",
        tilt(&w, id)
    );
    assert!(below > 0.2, "its foot is in the hole: {below:.2} m down");
}

#[test]
fn a_crate_is_still_held_level() {
    let mut w = world();
    let ada = w.avatar_of("ada");
    wait(&mut w, ada, 60, DVec3::ZERO);
    let centre = w.world.bodies[w.index(ada).unwrap()].structure.position[0];
    let up = character::plumb(&w.world, centre);
    let (east, _) = creeer_terrain::geology::deposits::tangent_frame(up);
    let id = w.add_item(
        Item::Piece(
            Piece::solid(
                "spruce_green",
                800.0,
                Form::Block {
                    size: DVec3::splat(0.3),
                },
            ),
            Vec::new(),
        ),
        w.ground_under(centre + east * 0.9) + up * 0.16,
        false,
    );
    wait(&mut w, ada, 30, DVec3::ZERO);
    let p = w.world.bodies[w.index(id).unwrap()].structure.position[0];
    #[allow(clippy::cast_precision_loss)]
    let seen = w.tick as f64;
    let target = Target {
        body: Some(id),
        at: p + up * 0.15,
        seen,
    };
    w.apply(&Command::Grab(target), Some(ada)).unwrap();
    // 21.6 kg (a 0.3 m block of green spruce): more than one hand carries, held in both.
    assert!(
        !w.carrying.get(&ada).expect("held").one_hand,
        "in both hands"
    );
}

#[test]
#[ignore = "probe: a pole left leaning in a narrow hole"]
fn probe_a_pole_leaning_in_a_hole() {
    let (mut w, ada, id, east, mouth) = ada_and_pole();
    let up = mouth.normalize();
    for lean in [10.0f64, 30.0, 45.0, 60.0] {
        let i = w.index(id).unwrap();
        let (s, c) = creeer_math::det::sin_cos(lean.to_radians());
        let axis = (up * c - east * s).normalize();
        // Its foot 0.3 m down the hole, against its east wall.
        let foot = mouth - up * 0.3 + east * 0.08;
        let body = &mut w.world.bodies[i];
        body.structure.orientation[0] = DQuat::from_rotation_arc(DVec3::X, -axis);
        body.structure.position[0] = foot + axis * 1.5;
        body.structure.velocity[0] = DVec3::ZERO;
        body.structure.angular_velocity[0] = DVec3::ZERO;
        body.solver.reset_history(&body.structure);
        body.wake();
        let mut most = 0.0f64;
        for _ in 0..240 {
            wait(&mut w, ada, 1, DVec3::ZERO);
            let s = &w.world.bodies[w.index(id).unwrap()].structure;
            most = most.max(s.velocity[0].length());
        }
        eprintln!("lean {lean}°: fastest {most:.2} m/s");
    }
}
