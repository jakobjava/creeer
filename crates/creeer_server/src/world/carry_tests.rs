//! A player carries what they reach in both hands and throws it (G1.4f): the hands bring it in
//! and hold it where people hold loads, it drops when it pulls harder than they hold up, a
//! throw sends it at the speed an overarm throw of its mass gets, the thrower pushed back.

use super::looks::Drawn;
use super::tests::world;
use super::*;
use creeer_net::message::Target;
use creeer_physics::world::shape::Shape;
use creeer_sim::carry;

/// A world with ada standing, a box of `mass` (kg, 0.4 m on a side) on the ground beside her,
/// her avatar, the box, and the way east.
fn ada_and_box(mass: f64) -> (ServerWorld, EntityId, EntityId, DVec3) {
    let mut w = world();
    let ada = w.avatar_of("ada");
    wait(&mut w, ada, 60, DVec3::ZERO);
    let centre = w.world.bodies[w.index(ada).unwrap()].structure.position[0];
    let up = character::plumb(&w.world, centre);
    let (east, _) = creeer_terrain::geology::deposits::tangent_frame(up);
    let at = w.ground_under(centre + east * 1.0) + up * 0.21;
    let mut body = Body::rigid(
        Shape::Box {
            half: DVec3::splat(0.2),
        },
        mass,
        at,
        DT,
    );
    body.structure.orientation[0] = crate::things::upright(at);
    body.friction = 0.6;
    let id = w.add_body(body, None);
    wait(&mut w, ada, 30, DVec3::ZERO);
    (w, ada, id, east)
}

fn wait(w: &mut ServerWorld, ada: EntityId, n: u32, walk: DVec3) {
    let input = AvatarInput {
        walk,
        ..AvatarInput::default()
    };
    for _ in 0..n {
        w.steer(ada, &input);
        w.step();
    }
}

/// The top of `id` as it is now.
fn top(w: &ServerWorld, id: EntityId) -> Target {
    let p = w.world.bodies[w.index(id).unwrap()].structure.position[0];
    #[allow(clippy::cast_precision_loss)]
    let seen = w.tick as f64;
    Target {
        body: Some(id),
        at: p + p.normalize() * 0.2,
        seen,
    }
}

fn at(w: &ServerWorld, id: EntityId) -> DVec3 {
    w.world.bodies[w.index(id).unwrap()].structure.position[0]
}

#[test]
fn a_box_is_brought_in_and_held_at_the_elbows_and_carried_along() {
    let (mut w, ada, id, east) = ada_and_box(20.0);
    w.apply(&Command::Grab(top(&w, id)), Some(ada)).unwrap();
    assert_eq!(w.carried(ada), Some(id));
    wait(&mut w, ada, 90, DVec3::ZERO);
    // Up off the ground, at the elbows' height (its top's middle where the hands hold it), at
    // least a forearm ahead, and clear of the body (none of it within the body).
    let centre = at(&w, ada);
    let up = character::plumb(&w.world, centre);
    let grabbed = at(&w, id) + up * 0.2;
    let held = carry::held(centre, up, DVec3::ZERO);
    assert!(
        (grabbed - held).dot(up).abs() < 0.03,
        "{} m off the elbows' height",
        (grabbed - held).dot(up)
    );
    let out = (grabbed - centre).reject_from(up).length();
    assert!(out > carry::forearm() - 0.03, "{out} m ahead");
    assert!(w.terrain.density(at(&w, id)) > 0.5, "off the ground");
    let (a, b) = (w.index(ada).unwrap(), w.index(id).unwrap());
    let into = (w.world.touching(b, 0.0).iter())
        .filter(|t| t.other.is_some_and(|o| o.0 == a))
        .map(|t| -t.gap)
        .fold(0.0, f64::max);
    assert!(into < 0.005, "{into} m into the body");
    // Walked east 3 s: carried along, still in hand.
    wait(&mut w, ada, 180, east * 1.2);
    assert_eq!(w.carried(ada), Some(id));
    assert!((at(&w, id) - at(&w, ada)).length() < 0.8);
    // Let go: it falls.
    w.apply(&Command::Drop, Some(ada)).unwrap();
    assert_eq!(w.carried(ada), None);
    assert!(
        w.world
            .joints()
            .iter()
            .all(|j| !j.joins(w.index(id).unwrap()))
    );
}

#[test]
fn a_load_heavier_than_the_hands_hold_up_drops() {
    // 70 kg: more than the 58.6 kg a man lifts to his elbows: not lifted.
    let (mut w, ada, id, _) = ada_and_box(70.0);
    let why = w.apply(&Command::Grab(top(&w, id)), Some(ada)).unwrap_err();
    assert!(why.contains("too heavy"), "{why}");
    // 40 kg: held.
    let (mut w, ada, id, _) = ada_and_box(40.0);
    w.apply(&Command::Grab(top(&w, id)), Some(ada)).unwrap();
    wait(&mut w, ada, 120, DVec3::ZERO);
    assert_eq!(w.carried(ada), Some(id), "held");
    // 30 kg more set on it: 70 kg in the hands, dropped.
    let p = at(&w, id);
    let mut more = Body::rigid(
        Shape::Box {
            half: DVec3::splat(0.2),
        },
        30.0,
        p + p.normalize() * 0.42,
        DT,
    );
    more.structure.orientation[0] = w.world.bodies[w.index(id).unwrap()].structure.orientation[0];
    w.add_body(more, None);
    wait(&mut w, ada, 60, DVec3::ZERO);
    assert_eq!(w.carried(ada), None, "dropped");
}

#[test]
fn a_throw_sends_it_as_an_overarm_throw_of_its_mass_and_pushes_back() {
    let (mut w, ada, id, east) = ada_and_box(5.0);
    w.apply(&Command::Grab(top(&w, id)), Some(ada)).unwrap();
    wait(&mut w, ada, 90, DVec3::ZERO);
    let (a, b) = (w.index(ada).unwrap(), w.index(id).unwrap());
    let before = (
        w.world.bodies[a].structure.velocity[0],
        w.world.bodies[b].structure.velocity[0],
    );
    let way = (east + at(&w, ada).normalize()).normalize();
    w.apply(&Command::Throw(way), Some(ada)).unwrap();
    assert_eq!(w.carried(ada), None);
    let after = (
        w.world.bodies[a].structure.velocity[0],
        w.world.bodies[b].structure.velocity[0],
    );
    // Its speed from the thrower, along the way: the law's for 5 kg (about 6.7 m/s).
    let relative = (after.1 - after.0).dot(way);
    let want = carry::throw_speed(5.0);
    assert!(
        (relative - want).abs() < 0.05 * want,
        "{relative} m/s against {want}"
    );
    // Momentum kept: what the box gained, the thrower lost.
    let gained = (after.1 - before.1) * 5.0;
    let lost = (after.0 - before.0) * character::MASS;
    assert!(
        (gained + lost).length() < 1e-6 * gained.length(),
        "{gained} and {lost}"
    );
}

#[test]
fn only_a_body_in_reach_is_grabbed_and_one_at_a_time() {
    let (mut w, ada, id, _) = ada_and_box(10.0);
    // The ground: nothing to hold. Another player: not carried.
    let ground = Target {
        body: None,
        ..top(&w, id)
    };
    assert!(w.apply(&Command::Grab(ground), Some(ada)).is_err());
    let bob = w.avatar_of("bob");
    let him = Target {
        body: Some(bob),
        at: at(&w, bob),
        ..top(&w, id)
    };
    assert!(w.apply(&Command::Grab(him), Some(ada)).is_err());
    // With the axe in hand: put away. Then the hands are full.
    w.apply(&Command::Wield { tool: Some(2) }, Some(ada))
        .unwrap();
    w.apply(&Command::Grab(top(&w, id)), Some(ada)).unwrap();
    let Drawn::Person {
        wields, carries, ..
    } = w.drawn(w.index(ada).unwrap())
    else {
        panic!("a person");
    };
    assert_eq!((wields, carries), (None, Some(id)));
    let why = w.apply(&Command::Grab(top(&w, id)), Some(ada)).unwrap_err();
    assert!(why.contains("hands full"), "{why}");
    assert!(w.apply(&Command::Throw(DVec3::ZERO), Some(ada)).is_err());
}

#[test]
fn a_load_saved_in_hand_is_let_go_when_loaded() {
    let (mut w, ada, id, _) = ada_and_box(10.0);
    w.apply(&Command::Grab(top(&w, id)), Some(ada)).unwrap();
    wait(&mut w, ada, 30, DVec3::ZERO);
    let (a, b) = (w.index(ada).unwrap(), w.index(id).unwrap());
    assert!(w.world.passes(a, b), "held, it passes the body");
    let back = ServerWorld::from_save(w.data.clone(), w.to_save(), super::tests::terrain());
    assert_eq!(back.carried(ada), None);
    let (a, b) = (back.index(ada).unwrap(), back.index(id).unwrap());
    assert!(!back.world.passes(a, b), "let go, it meets the body again");
}

/// Walking off with loads up to the most the hands lift: none tips her over (her balance
/// holds: the hands give out first, over about 45 kg walking); 30 kg is carried along.
#[test]
fn the_hands_give_out_before_a_load_tips_the_body_over() {
    for mass in [30.0, 45.0, 55.0] {
        let (mut w, ada, id, east) = ada_and_box(mass);
        w.apply(&Command::Grab(top(&w, id)), Some(ada)).unwrap();
        let mut fell = None;
        for t in 0..360 {
            let walking = if t >= 120 { east * 1.2 } else { DVec3::ZERO };
            wait(&mut w, ada, 1, walking);
            if w.people.get(&ada).is_some_and(people::Person::fallen) && fell.is_none() {
                fell = Some(t);
            }
        }
        let v = w.world.bodies[w.index(ada).unwrap()].structure.velocity[0].length();
        assert_eq!(fell, None, "{mass} kg tipped her over");
        if mass < 40.0 {
            assert_eq!(w.carried(ada), Some(id), "{mass} kg carried");
            assert!((v - 1.2).abs() < 0.05, "walking at {v} m/s");
        }
    }
}

/// Walking off as soon as it is taken, over where it lay: lifted and carried along, not
/// dropped (held from the world point instead, the body speeding up shook it out, G1.4f).
#[test]
fn walking_off_at_once_the_load_is_lifted_and_carried_along() {
    let (mut w, ada, id, east) = ada_and_box(15.0);
    w.apply(&Command::Grab(top(&w, id)), Some(ada)).unwrap();
    wait(&mut w, ada, 120, east * 1.2);
    assert_eq!(w.carried(ada), Some(id));
    assert!(w.terrain.density(at(&w, id)) > 0.5, "off the ground");
}

#[test]
fn the_sandboxs_box_is_one_to_carry() {
    let (mut w, ada, _, east) = ada_and_box(10.0);
    let centre = at(&w, ada);
    let ground = w.ground_under(centre - east * 1.0);
    w.apply(&Command::Box(ground + ground.normalize() * 0.3), Some(ada))
        .unwrap();
    let id = *w.ids().last().unwrap();
    wait(&mut w, ada, 60, DVec3::ZERO);
    w.apply(&Command::Grab(top(&w, id)), Some(ada)).unwrap();
    wait(&mut w, ada, 90, DVec3::ZERO);
    assert_eq!(w.carried(ada), Some(id));
}
