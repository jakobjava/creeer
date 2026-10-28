//! Fastening while holding (G2.10, ADR-0036): a part held steady where it lies is nailed by
//! the free hand, or tacked by a lashing's first turns; let go, the tack holds it, and both
//! hands finish the lashing.

use super::building_tests::{centre, fasten, pole, wait};
use super::tests::world;
use super::*;
use creeer_math::glam::DQuat;
use creeer_net::message::Target;

/// Ada, a pole lying across her way 0.6 m before her (the one she will hold), and on it, along
/// her way, another crossing it 10 cm from its near end: their ids and where they cross.
fn across() -> (ServerWorld, EntityId, EntityId, EntityId, DVec3) {
    let mut w = world();
    w.creative = true;
    let ada = w.avatar_of("ada");
    wait(&mut w, ada, 60);
    let me = centre(&w, ada);
    let up = character::plumb(&w.world, me);
    let (east, _) = creeer_terrain::geology::deposits::tangent_frame(up);
    // A rod lies along its own x.
    let lying = |axis: DVec3| {
        let side = up.reject_from(axis).normalize();
        DQuat::from_mat3(&creeer_math::glam::DMat3::from_cols(
            axis,
            side,
            axis.cross(side),
        ))
    };
    let lay = |w: &mut ServerWorld, id, at: DVec3, axis: DVec3| {
        let i = w.index(id).expect("there");
        let b = &mut w.world.bodies[i];
        (b.structure.position[0], b.structure.orientation[0]) = (at, lying(axis));
        b.solver.reset_history(&b.structure);
    };
    let at = w.ground_under(me + east * 0.6) + up * 0.03;
    let held = w.add_item(pole(), at, true);
    lay(&mut w, held, at, east.cross(up).normalize());
    // The other from on the held one to the ground beyond (the ground may rise or fall).
    let on = at + up * 0.051;
    let along = (w.ground_under(me + east * 2.1) + up * 0.03 - on).normalize();
    let other = w.add_item(pole(), on + along * 0.7, true);
    lay(&mut w, other, on + along * 0.7, along);
    wait(&mut w, ada, 90);
    let j = w.index(other).expect("there");
    let touch = (w
        .world
        .touching(w.index(held).expect("there"), 0.005)
        .into_iter())
    .find(|t| t.other.is_some_and(|(k, _)| k == j))
    .expect("they cross");
    (w, ada, held, other, touch.point)
}

/// Ada's hold of `part` where it lies now, held steady or not (G2.10).
fn held_where_it_lies(w: &ServerWorld, ada: EntityId, part: EntityId, steady: bool) -> AvatarInput {
    let s = &w.world.bodies[w.index(part).expect("there")].structure;
    AvatarInput {
        hold: Some(creeer_net::message::Hold {
            offset: s.position[0] - centre(w, ada),
            turn: s.orientation[0],
            steady,
        }),
        ..AvatarInput::default()
    }
}

/// Ada takes `part` at `at` in her hands, and the hold that keeps it steady where it lies.
fn taken_steady(w: &mut ServerWorld, ada: EntityId, part: EntityId, at: DVec3) -> AvatarInput {
    let target = Target {
        body: Some(part),
        at,
        seen: w.tick as f64,
    };
    w.apply(&Command::Grab(target), Some(ada)).expect("taken");
    held_where_it_lies(w, ada, part, true)
}

/// Each holding joint of structure `id`: what its fasteners hold together (N).
fn joints_hold(w: &ServerWorld, id: EntityId) -> Vec<f64> {
    let s = &w.world.bodies[w.index(id).expect("there")].structure;
    (0..s.element_count())
        .filter_map(|e| s.joint_withdrawal(e))
        .collect()
}

/// Steps `ticks`, Ada holding with `hold` while her hands hold anything; how far `part` went
/// from where it was while it was still apart from `other`.
fn held_for(
    w: &mut ServerWorld,
    (ada, hold): (EntityId, &AvatarInput),
    (part, other): (EntityId, EntityId),
    ticks: u32,
    mut each: impl FnMut(&mut ServerWorld, u32),
) -> f64 {
    let start = centre(w, part);
    let mut farthest = 0.0_f64;
    for t in 0..ticks {
        each(w, t);
        let input = if w.carried(ada).is_some() {
            *hold
        } else {
            AvatarInput::default()
        };
        w.steer(ada, &input);
        w.step();
        if w.index(other).is_some() {
            farthest = farthest.max((centre(w, part) - start).length());
        }
    }
    farthest
}

#[test]
fn a_part_held_steady_is_nailed_where_it_is_held_then_let_go() {
    // In creative, nails at hand: Ada takes the pole before her feet where the other crosses it
    // (she stoops to it).
    let (mut w, ada, part, other, at) = across();
    let steady = taken_steady(&mut w, ada, part, at);
    let moving = held_where_it_lies(&w, ada, part, false);
    w.steer(ada, &moving);
    w.step();
    for fastener in ["nail_100", "bast_lashing"] {
        let why = fasten(&mut w, ada, part, at, fastener).expect_err("not held steady");
        assert!(why.contains("steady"), "{fastener}: {why}");
    }
    // Held steady where it lies, a nail is driven with the other hand in its time (6 s).
    let farthest = held_for(&mut w, (ada, &steady), (part, other), 400, |w, t| {
        if t == 30 {
            fasten(w, ada, part, at, "nail_100").expect("begins");
        }
    });
    assert!(farthest < 0.01, "moved {farthest} m while held steady");
    assert!(w.index(other).is_none(), "nailed: {:?}", w.take_notes());
    assert!(
        w.carried(ada).is_none(),
        "the nail holds it: the hands let go"
    );
}

/// A bast lashing's two turns hold 1,920 N apart, its four 3,840 N (`creeer_building`'s tests).
#[test]
fn a_lashing_tacked_while_held_holds_the_part_and_both_hands_finish_it() {
    let (mut w, ada, part, other, at) = across();
    let steady = taken_steady(&mut w, ada, part, at);
    // Held steady, one hand holds it while the other makes the first two turns (15 s).
    let farthest = held_for(&mut w, (ada, &steady), (part, other), 30 + 905, |w, t| {
        if t == 30 {
            fasten(w, ada, part, at, "bast_lashing").expect("tacking begins");
        }
    });
    assert!(farthest < 0.01, "moved {farthest} m while held steady");
    let notes = w.take_notes();
    assert!(w.index(other).is_none(), "tacked: {notes:?}");
    assert!(
        w.carried(ada).is_none(),
        "the tack holds it: the hands let go"
    );
    let close = |a: f64, b: f64| (a / b - 1.0).abs() < 1e-9;
    let tack = joints_hold(&w, part);
    assert!(tack.len() == 1 && close(tack[0], 1920.0), "{tack:?}");
    // Taken again, it cannot be finished: that takes both hands.
    taken_steady(&mut w, ada, part, at);
    let why = fasten(&mut w, ada, part, at, "bast_lashing").expect_err("in hand");
    assert!(why.contains("both hands"), "{why}");
    w.apply(&Command::Drop, Some(ada)).expect("put down");
    // Hands free, the rest of its turns are drawn in the rest of its time (15 s).
    fasten(&mut w, ada, part, at, "bast_lashing").expect("finishing begins");
    wait(&mut w, ada, 890);
    assert!(close(joints_hold(&w, part)[0], 1920.0), "not finished yet");
    wait(&mut w, ada, 20);
    let whole = joints_hold(&w, part);
    assert!(
        whole.len() == 1 && close(whole[0], 3840.0),
        "{whole:?}: {:?}",
        w.take_notes()
    );
}
