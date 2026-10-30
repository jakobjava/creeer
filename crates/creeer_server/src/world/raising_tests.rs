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
/// deep) and widened along the pole into a slot, a 3 m green spruce pole lying in the slot with
/// its foot 20 cm down the hole, ada at its top end: the world, ada, the pole, the way east, and
/// the hole's mouth.
///
/// A pole 12 cm thick could not lie with its foot 20 cm down a 15 cm-radius hole on level
/// ground: laid there (as this test did before G2.10), its top on the ground 3 m west, it lay
/// up to 28 cm into the ground just past the rim, was pushed out as it settled, and its foot
/// came to rest 7 cm above the mouth, over the hole, not in it. A pole lies at rest with its foot
/// in a hole only if its middle (1.5 m along) lies over what holds it: in a pit 1.6 m across, or
/// in a slot dug beside the hole along the pole, as linemen setting a pole by hand dig a slot or
/// ramp beside its hole, on the side it is raised from, for its butt to slide down into the hole
/// as it is walked up (the user, 2026-10-29). The slot: the pole rising 8° to the west, the
/// slot's floor under it out to where it clears the ground (2.1 m: its middle over the floor),
/// open above it.
pub(super) fn ada_and_pole() -> (ServerWorld, EntityId, EntityId, DVec3, DVec3) {
    ada_and_pole_dug(true)
}

/// [`ada_and_pole`], the slot dug (`slot`) or not: without it the hole alone, as G2.9's rim
/// tests measured it, the pole laid as it was then (into the ground beside the hole; they set
/// it themselves).
pub(super) fn ada_and_pole_dug(slot: bool) -> (ServerWorld, EntityId, EntityId, DVec3, DVec3) {
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
    // Its foot 20 cm down the hole, the pole rising 8° to the west of it; the slot as wide as
    // the hole, its floor 1 cm under the pole, out to where its underside clears the ground by
    // 2 cm. (A slot 14 cm across, finer than the terrain's 12.5 cm cells and its 37.5 cm rounds
    // draw, wedged the pole: its steps found it with up to 239 kJ.)
    let foot = mouth - mouth.normalize() * 0.2;
    let (s8, c8) = creeer_math::det::sin_cos(8f64.to_radians());
    let west = -east * c8 + up * s8;
    let clears = |x: f64| {
        let p = foot + west * x - up * 0.06;
        let (d, g) = w.terrain.density_gradient(p);
        d / g.length().max(1e-9) > 0.02
    };
    let out = (0..60)
        .map(|k| f64::from(k) * 0.05)
        .find(|&x| x > 0.3 && clears(x))
        .unwrap_or(3.0);
    let floor = up * (0.15 - 0.07);
    if slot {
        w.terrain.edit(Edit::Dig(Brush::Capsule {
            a: foot + floor,
            b: foot + west * out + floor,
            radius: 0.15,
        }));
    }
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
    // the rest of it lying west in the slot and past it.
    let top = if slot {
        foot + west * 3.0
    } else {
        let lying = w.ground_under(mouth - east * 3.0) + up * 0.06;
        foot + (lying - foot).normalize() * 3.0
    };
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
pub(super) fn grab_the_top(w: &mut ServerWorld, ada: EntityId, id: EntityId, east: DVec3) {
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

/// The hold a player walks a pole up by, its foot in the hole: the grip turned about the pole's
/// foot towards the hands at head height before the body (0.75 m up, 0.45 m on), as far as the
/// arms reach (0.7 m from the shoulders, stooping as the server's hands do, `hands.rs`), the
/// foot left where it is. It lifts and turns the pole about its foot: the grip is never held
/// farther from the foot than it is along the pole, nor beyond the arms. (Before G2.10 the
/// hands were held at head height and the pole turned upright at once: 3.7 m from the foot of a
/// pole gripped 2.9 m up it. The grip pulled at its most, 575 N, and dragged the foot out of
/// the hole; see the test below. Turned towards the hands but past the arms' reach, the hands
/// held where they reached, and the foot was drawn 0.6 m up the slot.)
pub(super) fn reachable_hold(
    w: &ServerWorld,
    ada: EntityId,
    id: EntityId,
    east: DVec3,
) -> creeer_net::message::Hold {
    let s = &w.world.bodies[w.index(id).unwrap()].structure;
    let (q, centre) = (s.orientation[0], s.position[0]);
    // A rod's foot is its +x end, 1.5 m from its centre.
    let foot_local = DVec3::X * 1.5;
    let foot = centre + q * foot_local;
    let body = w.world.bodies[w.index(ada).unwrap()].structure.position[0];
    let up = body.normalize();
    let grip = w.carrying.get(&ada).map_or(DVec3::ZERO, |c| c.grip);
    let (from, length) = (
        (q * (grip - foot_local)).normalize(),
        (grip - foot_local).length(),
    );
    let hands = body + up * 0.75 + east * 0.45;
    let towards = DQuat::from_rotation_arc(from, (hands - foot).normalize());
    let shoulders = creeer_sim::reach::shoulders(body, up);
    let within = |p: DVec3| (p - creeer_sim::reach::stooped(shoulders, up, p)).length() <= 0.7;
    let at = (0..=20)
        .rev()
        .map(|k| foot + DQuat::IDENTITY.slerp(towards, f64::from(k) / 20.0) * from * length)
        .find(|&p| within(p))
        .unwrap_or(foot + from * length);
    // Turned so that the foot stays where it is: the foot-to-grip line onto foot-to-`at`.
    let turn = DQuat::from_rotation_arc(from, (at - foot).normalize()) * q;
    creeer_net::message::Hold {
        offset: at - body - turn * grip,
        turn,
        steady: false,
    }
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
#[ignore = "superseded by first-person placing (G2.10, 2026-10-30): an aimed pole is held at its \
            balance point and stood up (R), `build_flow_tests::a_heavy_pole_stood_up_and_set_in_its_hole`; \
            raising one too heavy to lift by its end returns with docs/future-improvements.md"]
fn walked_up_its_foot_in_a_hole_a_pole_comes_up_to_stand() {
    let (mut w, ada, id, east, mouth) = ada_and_pole();
    grab_the_top(&mut w, ada, id, east);
    // Raised at its top first, then walked under towards its foot, slowly, the hands as high
    // as the pole lets them before the body, the pole turning about its foot, as a player steers
    // it ([`reachable_hold`]; G2.10: brought in to the elbows by themselves, the hands drew its
    // foot out). As written before G2.10 this test passed only by a fault, with neither its
    // setup nor its hold possible: the pole laid 28 cm into the ground beside the hole came to
    // rest with its foot over the hole, not in it ([`ada_and_pole`]); the hold asked its grip
    // to be 3.7 m from its foot, gripped 2.9 m up it, and the grip pulled at its most (575 N)
    // and dragged the foot 0.77 m; and every impact under the person's feet stopped the pole
    // dead (an impact's budget left the legs' energy out, ADR-0005 G2.10), which is what held
    // its foot back. With the impacts fixed it was dragged out of the hole and never stood.
    let mut most_upright = 90.0f64;
    for round in 0..15 {
        for _ in 0..30 {
            let input = AvatarInput {
                walk: if round < 3 { DVec3::ZERO } else { east * 0.5 },
                hold: Some(reachable_hold(&w, ada, id, east)),
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
    let (mut w, ada, id, east, mouth) = ada_and_pole_dug(false);
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
