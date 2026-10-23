//! Playing over a network as it is: a client's bytes stay within the budget in typical play
//! (plan.md F5: 256 kbit/s), its own avatar is predicted with few corrections over a slow,
//! jittery, lossy link, and a structure bent in a crash is drawn bent.

use super::tests::Game;
use crate::world::ServerWorld;
use crate::world::tests::world;
use creeer_math::det;
use creeer_math::glam::DVec3;
use creeer_net::channel::BYTES_PER_SECOND;
use creeer_net::entity::Kind;
use creeer_net::link::Conditions;
use creeer_net::message::Command;
use creeer_net::quant::POSITION_SCALE;
use creeer_physics::world::Body;
use creeer_physics::world::shape::Shape;
use creeer_terrain::edits::{Brush, Edit, Edits};
use std::time::Duration;

fn frame_at(world: &ServerWorld, east: f64, north: f64) -> (DVec3, DVec3, DVec3) {
    let up = world.spawn().normalize();
    let (e, n) = creeer_terrain::geology::deposits::tangent_frame(up);
    (
        world.ground_under(world.spawn() + e * east + n * north),
        e,
        n,
    )
}

/// A typical scene: piles of crates and balls out to 80 m, two frames.
fn scene() -> ServerWorld {
    let mut w = world();
    for k in 0..120u32 {
        let angle = f64::from(k) * 2.4;
        let radius = 6.0 + f64::from(k % 25) * 3.0;
        let (s, c) = det::sin_cos(angle);
        let (at, ..) = frame_at(&w, radius * c, radius * s);
        let command = if k % 12 == 0 {
            Command::Ball(at + at.normalize() * 0.5)
        } else {
            Command::Crate(at + at.normalize() * 0.6)
        };
        // (Since G1.7, beyond the spawn's clearing, none is put into a trunk.)
        match w.apply(&command, None) {
            Err(e) if e != "a tree stands there" => panic!("{e}"),
            _ => {}
        }
    }
    for east in [15.0, -20.0] {
        let (at, ..) = frame_at(&w, east, 10.0);
        w.apply(&Command::Frame(at), None).unwrap();
    }
    w
}

#[test]
fn in_typical_play_a_client_is_sent_less_than_its_budget() {
    let mut game = Game::new(scene());
    // 40–50 ms each way, one packet in a hundred lost.
    let link = Conditions {
        latency: Duration::from_millis(40),
        jitter: Duration::from_millis(10),
        loss: 0.01,
    };
    game.join("ada", Some(link));
    game.run(60, |_| (DVec3::ZERO, false));
    // Twenty seconds of play: walking a wide circle through the crates, dropping a crate
    // every two seconds and digging every five.
    let ticks = 20 * 60;
    for t in 0..ticks {
        #[allow(clippy::cast_precision_loss)]
        let angle = t as f64 / 300.0;
        let walk = {
            let up = game.avatar(0).normalize();
            let (e, n) = creeer_terrain::geology::deposits::tangent_frame(up);
            let (s, c) = det::sin_cos(angle);
            (e * c + n * s) * 4.0
        };
        if t % 120 == 60 {
            let at = game.avatar(0) + walk.normalize() * 4.0;
            let at = game.server.world.ground_under(at);
            game.clients[0]
                .0
                .command(Command::Crate(at + at.normalize() * 1.5));
        }
        if t % 300 == 150 {
            let at = game
                .server
                .world
                .ground_under(game.avatar(0) + walk.normalize() * 8.0);
            let mut edits = Edits::default();
            edits.push(Edit::Dig(Brush::Sphere {
                center: at,
                radius: 2.0,
            }));
            game.clients[0].0.command(Command::Edit(edits.to_bytes()));
        }
        game.tick(|_| (walk, t % 180 == 90));
    }
    let relay = game.clients[0].1.as_ref().unwrap();
    let (down, packets) = relay.link.sent_to_client();
    let (up, _) = relay.link.sent_to_server();
    #[allow(clippy::cast_precision_loss)]
    let (down_rate, up_rate) = (down as f64 / 20.0, up as f64 / 20.0);
    // The budget: 256 kbit/s on the wire; the bucket's first fill (a fifth of a second's
    // worth) comes on top once.
    assert!(
        down_rate <= BYTES_PER_SECOND * 1.01,
        "{:.0} kbit/s to the client in {packets} packets",
        down_rate * 8e-3
    );
    assert!(up_rate <= 8_000.0, "{:.0} kbit/s from it", up_rate * 8e-3);
    // And the client has the world: every entity within its view, drawn where it is.
    let client = &game.clients[0].0;
    let world = &game.server.world;
    let centre = game.avatar(0);
    let in_view = world
        .world
        .bodies
        .iter()
        .filter(|b| (b.structure.position[0] - centre).length() < 550.0)
        .count();
    assert_eq!(client.kinds.len(), in_view);
    let poses = client.poses();
    let mut stale = 0;
    for (id, body) in world.ids().iter().zip(&world.world.bodies) {
        if body.sleep.asleep && (poses[id].position - body.structure.position[0]).length() > 0.01 {
            stale += 1;
        }
    }
    assert_eq!(stale, 0, "resting bodies drawn elsewhere than they rest");
    eprintln!(
        "typical play: {:.0} kbit/s down, {:.1} kbit/s up",
        down_rate * 8e-3,
        up_rate * 8e-3
    );
}

#[test]
fn over_a_slow_lossy_link_the_avatar_is_rarely_corrected() {
    let mut game = Game::new(world());
    // 100–130 ms each way, one packet in twenty lost.
    let link = Conditions {
        latency: Duration::from_millis(100),
        jitter: Duration::from_millis(30),
        loss: 0.05,
    };
    game.join("ada", Some(link));
    game.run(60, |_| (DVec3::ZERO, false));
    for t in 0..600u32 {
        let up = game.avatar(0).normalize();
        let (e, n) = creeer_terrain::geology::deposits::tangent_frame(up);
        let (s, c) = det::sin_cos(f64::from(t / 120) * 1.3);
        game.tick(|_| ((e * c + n * s) * 3.0, t % 150 == 75));
    }
    let c = game.clients[0].0.corrections();
    eprintln!("corrections: {c:?}");
    assert!(c.count > 200, "{c:?}");
    // (G1.2e: set back only where it was mispredicted. Set back to the quantised state at every
    // snapshot, 4 in 304 corrections were over a centimetre, the largest 1.4 cm; now none.)
    assert!(
        f64::from(c.over_a_centimetre) <= 0.01 * f64::from(c.count),
        "{c:?}"
    );
    assert!(c.largest < 0.02, "{c:?}");
}

#[test]
fn a_frame_bent_in_a_crash_is_drawn_bent() {
    let mut w = world();
    let (at, ..) = frame_at(&w, 6.0, 0.0);
    w.apply(&Command::Frame(at), None).unwrap();
    let frame = w.ids()[0];
    let mut game = Game::new(w);
    game.join("ada", None);
    game.run(60, |_| (DVec3::ZERO, false));
    let shape = |game: &Game| {
        let s = &game.server.world.world.bodies[game.server.world.index(frame).unwrap()].structure;
        let back = s.orientation[0].conjugate();
        s.position
            .iter()
            .map(|p| back * (*p - s.position[0]))
            .collect::<Vec<_>>()
    };
    let before = shape(&game);
    // 300 kg of steel dropped on it from a metre and a half (from a metre, at the spawn moved
    // in G1.3i, it bent 6 mm, too little to test; much more breaks members).
    let up = at.normalize();
    let mut block = Body::rigid(
        Shape::Box {
            half: DVec3::new(0.6, 0.3, 0.6),
        },
        300.0,
        at + up * 3.5,
        crate::world::DT,
    );
    block.structure.orientation[0] = creeer_math::glam::DQuat::from_rotation_arc(DVec3::Y, up);
    game.server.world.add_body(block, None);
    // Until it rests (and the last change has gone out: a few send rounds more). How long the
    // block takes to settle on the bent frame is chaotic: of eight drops a micrometre apart,
    // three (before G1.8) or four (after) were still rocking after 20 s, at millimetres a
    // second and losing energy all the while (one at rest after 25 s); contacts found anew
    // see-saw (ADR-0024).
    let i = game.server.world.index(frame).unwrap();
    let mut ticks = 0;
    while !game.server.world.world.bodies[i].sleep.asleep {
        game.tick(|_| (DVec3::ZERO, false));
        ticks += 1;
        assert!(ticks < 3000, "the frame never comes to rest");
    }
    game.run(30, |_| (DVec3::ZERO, false));
    let s = &game.server.world.world.bodies[i].structure;
    let bent = before
        .iter()
        .zip(shape(&game))
        .map(|(a, b)| (*a - b).length())
        .fold(0.0, f64::max);
    // Bent well past what a drawn shape may be off by, so drawing it bent is tested. (How far
    // is chaotic: a tumbling block's last bounce turns on rounding, and changes at the 1e-15
    // level put it anywhere in 2.7–3.6 cm.)
    assert!(
        bent > 2.0 * crate::replicate::SHAPE_TOLERANCE,
        "not bent: {bent} m"
    );
    assert!(s.members().iter().all(|m| !m.3), "a member broke");
    let Kind::Structure(drawn) = &game.clients[0].0.kinds[&frame] else {
        panic!("not a structure")
    };
    let back = s.orientation[0].conjugate();
    let mut worst = 0.0f64;
    for (k, p) in s.position.iter().enumerate() {
        let truth = back * (*p - s.position[0]);
        worst = worst.max((drawn.nodes[k] - truth).length());
    }
    // Within what it is told to (and quantised to).
    assert!(
        worst <= crate::replicate::SHAPE_TOLERANCE + 2.0 / POSITION_SCALE,
        "drawn {worst} m off"
    );
}

#[test]
fn a_frame_broken_in_a_crash_is_drawn_broken() {
    let mut w = world();
    let (at, ..) = frame_at(&w, 6.0, 0.0);
    w.apply(&Command::Frame(at), None).unwrap();
    let frame = w.ids()[0];
    let mut game = Game::new(w);
    game.join("ada", None);
    game.run(60, |_| (DVec3::ZERO, false));
    // 300 kg dropped from 3 m: members break (and buckle, splitting).
    let up = at.normalize();
    let mut block = Body::rigid(
        Shape::Box {
            half: DVec3::new(0.6, 0.3, 0.6),
        },
        300.0,
        at + up * 5.0,
        crate::world::DT,
    );
    block.structure.orientation[0] = creeer_math::glam::DQuat::from_rotation_arc(DVec3::Y, up);
    game.server.world.add_body(block, None);
    game.run(300, |_| (DVec3::ZERO, false));
    let i = game.server.world.index(frame).unwrap();
    let s = &game.server.world.world.bodies[i].structure;
    let members = s.members();
    #[allow(clippy::cast_possible_truncation)]
    let broken: std::collections::BTreeSet<u32> = members
        .iter()
        .enumerate()
        .filter(|(_, m)| m.3)
        .map(|(k, _)| k as u32)
        .collect();
    assert!(!broken.is_empty(), "nothing broke");
    let client = &game.clients[0].0;
    let Kind::Structure(drawn) = &client.kinds[&frame] else {
        panic!("not a structure")
    };
    assert_eq!(drawn.nodes.len(), s.node_count());
    #[allow(clippy::cast_possible_truncation)]
    let pairs: Vec<[u32; 2]> = members.iter().map(|m| [m.0 as u32, m.1 as u32]).collect();
    assert_eq!(drawn.members, pairs);
    assert_eq!(client.broken.get(&frame), Some(&broken));
}
