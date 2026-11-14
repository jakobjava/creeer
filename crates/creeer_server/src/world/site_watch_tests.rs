//! What a creative server shows of what is built (ADR-0037): each element's share of what
//! breaks it, and a failure told in words to the players near, with what it carried; nothing on
//! a survival server.

use super::super::building_tests::{centre, fasten, pole, wait};
use super::super::tests::world;
use super::*;
use creeer_math::glam::DQuat;

/// Ada beside two crossed poles nailed into one structure (`creative` or not): the structure.
fn nailed(creative: bool) -> (ServerWorld, EntityId, EntityId) {
    let mut w = world();
    w.creative = true;
    let ada = w.avatar_of("ada");
    wait(&mut w, ada, 60);
    let me = centre(&w, ada);
    let up = character::plumb(&w.world, me);
    let (east, _) = tangent_frame(up);
    let ground = w.ground_under(me + east * 1.2);
    let below = w.add_item(pole(), ground + up * 0.03, true);
    let above = w.add_item(pole(), ground + up * 0.09, true);
    let i = w.index(above).expect("there");
    let s = &mut w.world.bodies[i].structure;
    s.orientation[0] = DQuat::from_axis_angle(up, std::f64::consts::FRAC_PI_2) * s.orientation[0];
    wait(&mut w, ada, 90);
    let j = w.index(below).expect("there");
    let at = (w
        .world
        .touching(w.index(above).expect("there"), 0.005)
        .into_iter())
    .find(|t| t.other.is_some_and(|(k, _)| k == j))
    .expect("they cross")
    .point;
    fasten(&mut w, ada, above, at, "nail_100").expect("begins");
    wait(&mut w, ada, 200);
    assert!(w.index(below).is_none(), "nailed");
    w.creative = creative;
    let _ = w.take_notes();
    (w, ada, above)
}

/// The joint's element in structure `id`.
fn joint(w: &ServerWorld, id: EntityId) -> usize {
    let s = &w.world.bodies[w.index(id).expect("built")].structure;
    (0..s.element_count())
        .find(|&e| s.member(e).is_none())
        .expect("a joint")
}

#[test]
fn a_joint_that_breaks_is_told_with_what_it_carried() {
    let (mut w, ada, built) = nailed(true);
    wait(&mut w, ada, 2);
    let seen = w.inspection(true);
    let (_, shares) = (seen.shares.iter())
        .find(|(id, _)| *id == built)
        .expect("its shares");
    assert!(shares.iter().all(|s| (0.0..1.0).contains(s)), "{shares:?}");
    assert!(seen.failures.is_empty());
    // The nail fails (as an overload would break it: severed).
    let e = joint(&w, built);
    let i = w.index(built).expect("built");
    w.world.bodies[i].structure.sever(e);
    wait(&mut w, ada, 1);
    let seen = w.inspection(false);
    assert_eq!(seen.failures.len(), 1, "{:?}", seen.failures);
    let f = &seen.failures[0];
    assert!(f.joint && f.structure == built);
    assert!(f.words.starts_with("the joint between a "), "{}", f.words);
    assert!(f.words.contains("kN"), "{}", f.words);
    // Where the poles crossed, and Ada was told.
    assert!((f.at - centre(&w, built)).length() < 1.5, "{:?}", f.at);
    let notes = w.take_notes();
    assert!(
        notes.iter().any(|(who, n)| *who == ada && *n == f.words),
        "{notes:?}"
    );
}

#[test]
fn a_survival_server_shows_nothing() {
    let (mut w, ada, built) = nailed(false);
    let e = joint(&w, built);
    let i = w.index(built).expect("built");
    w.world.bodies[i].structure.sever(e);
    wait(&mut w, ada, 2);
    assert_eq!(w.inspection(true), Inspection::default());
    assert!(w.take_notes().is_empty());
}
