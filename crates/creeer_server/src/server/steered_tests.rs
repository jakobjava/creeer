//! The player's own load steered over a slow and lossy link (G2.10, ADR-0036): drawn moving
//! the frame the hold changes, gliding there at the hands' speed, and where the server has it
//! once its hands have brought it there, with no jump between; pushed into the ground (blocked),
//! drawn in it no longer than the round trip and their glide take.

use super::Game;
use creeer_math::glam::{DQuat, DVec3};
use creeer_net::entity::EntityId;
use creeer_net::link::Conditions;
use creeer_net::message::{Command, Hold, Target};
use creeer_sim::character;
use std::time::Duration;

const TICK: f64 = 1.0 / 60.0;

/// Ada over the slow link of the reach tests (100 ms each way, 30 ms jitter, 5 % lost) holding
/// a 12 kg box she took from the ground, steering it where it is drawn: the game, her avatar,
/// the box, the hold, the up and the way east.
fn holding() -> (Game, EntityId, EntityId, Hold, DVec3, DVec3) {
    let mut game = Game::new(crate::world::tests::world());
    let link = Conditions {
        latency: Duration::from_millis(100),
        jitter: Duration::from_millis(30),
        loss: 0.05,
    };
    game.join("ada", Some(link));
    game.run(120, super::still);
    let centre = game.avatar(0);
    let up = character::plumb(&game.server.world.world, centre);
    let (east, _) = creeer_terrain::geology::deposits::tangent_frame(up);
    let at = game.server.world.ground_under(centre + east * 1.0) + up * 0.21;
    let shape = creeer_physics::world::shape::Shape::Box {
        half: DVec3::splat(0.2),
    };
    let mut body = creeer_physics::world::Body::rigid(shape, 12.0, at, crate::world::DT);
    body.friction = 0.6;
    let id = game.server.world.add_body(body, None);
    game.run(30, super::still);
    let client = &mut game.clients[0].0;
    let drawn = client.poses()[&id].position;
    let aimed = client.aim(drawn + up * 3.0, -up, 5.0).expect("the box");
    client.command(Command::Grab(Target {
        body: aimed.body,
        at: aimed.point,
        seen: aimed.seen,
    }));
    game.run(120, super::still);
    let ada = game.clients[0].0.welcome.as_ref().expect("joined").avatar;
    assert_eq!(game.server.world.carried(ada), Some(id));
    // Steered from where it is drawn (as `holding.rs` takes it), until it rests there.
    let hold = steered_from(&game, ada, id);
    game.clients[0].0.set_hold(Some(hold));
    game.run(120, super::still);
    (game, ada, id, hold, up, east)
}

/// The hold where ada's client draws the box now.
fn steered_from(game: &Game, ada: EntityId, id: EntityId) -> Hold {
    let poses = game.clients[0].0.poses();
    Hold {
        offset: poses[&id].position - poses[&ada].position,
        turn: poses[&id].orientation,
        steady: false,
    }
}

/// The box as ada's client draws it: from her avatar, and turned.
fn drawn(game: &Game, ada: EntityId, id: EntityId) -> (DVec3, DQuat) {
    let poses = game.clients[0].0.poses();
    (
        poses[&id].position - poses[&ada].position,
        poses[&id].orientation,
    )
}

#[test]
fn the_own_load_answers_the_hold_within_a_frame_and_comes_to_the_servers() {
    let (mut game, ada, id, hold, up, east) = holding();
    let (before, turned) = drawn(&game, ada, id);
    // Steered 15 cm east and 10 cm up, turned 0.3 rad about the up.
    let moved = east * 0.15 + up * 0.1;
    let turn = DQuat::from_axis_angle(up, 0.3);
    let steered = Hold {
        offset: hold.offset + moved,
        turn: (turn * hold.turn).normalize(),
        ..hold
    };
    game.clients[0].0.set_hold(Some(steered));
    game.run(1, super::still);
    let (first, _) = drawn(&game, ada, id);
    let started = (first - before).dot(moved.normalize());
    // Then: there at the hands' speed (and turned), the server's shown a round trip on (when its
    // own pose, the drawn less the correction, first moves 1 cm).
    let (mut late, mut jump, mut there, mut ticks) = (None, 0.0_f64, None, 1);
    let mut last = first;
    while ticks < 150 {
        game.run(1, super::still);
        ticks += 1;
        let (now, now_turn) = drawn(&game, ada, id);
        let (by, _) = game.clients[0].0.steered_by();
        let server = now - by;
        if late.is_none() && (server - before).length() > 0.01 {
            late = Some(ticks);
        }
        let turned_by = (now_turn * turned.conjugate()).angle_between(DQuat::IDENTITY);
        if there.is_none()
            && (now - (before + moved)).length() < 0.002
            && (turned_by - 0.3).abs() < 0.002
        {
            there = Some(ticks);
        }
        jump = jump.max((now - last).length());
        last = now;
    }
    let (by, by_turn) = game.clients[0].0.steered_by();
    println!(
        "steered {:.0} mm and 0.3 rad: drawn moving {:.1} mm in the first frame, there in {:.0} \
         ms; the server's shown {:.0} ms on; a frame moved {:.1} mm at most; at 2.5 s the \
         correction {:.2} mm, {:.4} rad",
        moved.length() * 1e3,
        started * 1e3,
        f64::from(there.unwrap_or(0)) * TICK * 1e3,
        f64::from(late.unwrap_or(0)) * TICK * 1e3,
        jump * 1e3,
        by.length() * 1e3,
        by_turn
    );
    // Answering in the first frame, at the hands' speed (2 m/s: 33 mm a tick).
    let step = creeer_sim::carry::FASTEST * TICK;
    assert!(started > 0.8 * step, "{started} m");
    let glide = moved.length() / creeer_sim::carry::FASTEST;
    assert!(
        there.is_some_and(|t| f64::from(t) * TICK < glide + 4.0 * TICK),
        "there at {there:?}"
    );
    assert!(
        late.is_some_and(|t| t > 6),
        "the server's shown at once: {late:?}"
    );
    // Converged on the server's, and never faster than the hands on the way.
    assert!(by.length() < 1e-3 && by_turn < 1e-3, "{by} {by_turn}");
    assert!(jump < step + 0.005, "a jump of {jump} m");
}

#[test]
fn a_load_pushed_into_the_ground_is_drawn_in_it_no_longer_than_the_hands_take() {
    let (mut game, ada, id, hold, up, _) = holding();
    // Set down on the ground before her (the hands bring it there), then pushed 15 cm into it.
    let box_at = |game: &Game| {
        let w = &game.server.world;
        w.world.bodies[w.index(id).expect("there")]
            .structure
            .position[0]
    };
    let ground = game.server.world.ground_under(box_at(&game));
    let down = (box_at(&game) - ground).dot(up) - 0.2;
    let set = Hold {
        offset: hold.offset - up * down,
        ..hold
    };
    game.clients[0].0.set_hold(Some(set));
    game.run(180, super::still);
    let rest = steered_from(&game, ada, id);
    game.clients[0].0.set_hold(Some(Hold {
        offset: rest.offset - up * 0.15,
        ..rest
    }));
    game.run(1, super::still);
    let (pushed, _) = game.clients[0].0.steered_inputs();
    // How far below the server's box (as drawn) the correction draws it, and since when it has
    // been under a centimetre.
    let below = |game: &Game| -game.clients[0].0.steered_by().0.dot(up);
    let (mut inside, mut deepest, mut round_trip, mut avatar) = (1, below(&game), None, 0.0_f64);
    for tick in 2..=120 {
        game.run(1, super::still);
        deepest = deepest.max(below(&game));
        if below(&game) > 0.01 {
            inside = tick;
        }
        // The push applied by the server at the moment drawn: the round trip as she sees it.
        let c = &game.clients[0].0;
        if round_trip.is_none() && c.steered_inputs().1 >= f64::from(pushed) {
            round_trip = Some(tick);
        }
        // (Her avatar as predicted, apart: the server's hands push her up as they press it down,
        // which her prediction does not know.)
        let w = &game.server.world;
        let real = w.world.bodies[w.index(ada).expect("there")]
            .structure
            .position[0];
        avatar = avatar.max((c.poses()[&ada].position - real).dot(up).abs());
    }
    let round_trip = f64::from(round_trip.expect("the push came round")) * TICK;
    let server = (box_at(&game) - game.server.world.ground_under(box_at(&game))).dot(up) - 0.2;
    let (inside, settles) = (f64::from(inside) * TICK, settling(deepest));
    println!(
        "pushed 150 mm into the ground: drawn {:.0} mm in it at most, over 1 cm for {:.0} ms (the \
         round trip {:.0} ms, their glide {:.0} ms); the server's box {:.1} mm above it; her \
         avatar drawn {:.0} mm off the server's",
        deepest * 1e3,
        inside * 1e3,
        round_trip * 1e3,
        settles * 1e3,
        server * 1e3,
        avatar * 1e3
    );
    // Drawn where steered at first, then in the ground for the round trip and as long as the
    // server's hands would take to bring it so far, and no longer.
    assert!(deepest > 0.1, "drawn where steered: {deepest} m in");
    assert!(
        inside <= round_trip + settles + 2.0 * TICK,
        "in the ground {inside} s"
    );
    assert!(
        server > -0.02,
        "the server's box {server} m into the ground"
    );
}

/// How long the server's hands take to bring a load `distance` (m) away there (s): straight,
/// at their speed (`carry::aimed_step`).
fn settling(distance: f64) -> f64 {
    distance / creeer_sim::carry::FASTEST
}
