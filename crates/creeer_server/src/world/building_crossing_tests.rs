//! Nailing a deck (G2.11 B): the nail goes the way a carpenter drives it, a board already nailed
//! to one stringer is nailed to the next, and a second nail at a crossing makes the joint of two.

use super::super::building_tests::{centre, fasten, wait};
use super::super::tests::world;
use super::*;
use creeer_math::glam::{DMat3, DQuat};

/// Kit part `name` laid with its middle at `at`, its length along `along` and its height up.
fn lay(w: &mut ServerWorld, name: &str, at: DVec3, along: DVec3, up: DVec3) -> EntityId {
    let item = w.kit_named(name).expect("a kit part");
    let id = w.add_item(item, at, false);
    let i = w.index(id).expect("there");
    let x = along.reject_from(up).normalize();
    let body = &mut w.world.bodies[i];
    body.structure.orientation[0] = DQuat::from_mat3(&DMat3::from_cols(x, up, x.cross(up)));
    body.solver.reset_history(&body.structure);
    id
}

/// Ada, two stringers on edge on the ground 0.8 m east of her, 0.4 m apart, running east, and a
/// deck board across them running north: their ids, the crossings' tops (on the board), the up.
struct Deck {
    w: ServerWorld,
    ada: EntityId,
    stringers: [EntityId; 2],
    board: EntityId,
    tops: [DVec3; 2],
    up: DVec3,
    east: DVec3,
}

fn deck() -> Deck {
    let mut w = world();
    let ada = w.avatar_of("ada");
    wait(&mut w, ada, 60);
    let me = centre(&w, ada);
    let up = character::plumb(&w.world, me);
    let (east, north) = creeer_terrain::geology::deposits::tangent_frame(up);
    let mid = me + east * 1.0;
    let rows = [-0.2, 0.2].map(|n| {
        let at = w.ground_under(mid + north * n);
        (at, w.ground_under(at))
    });
    let stringers = rows.map(|(at, _)| lay(&mut w, "stringer", at + up * 0.076, east, up));
    let over = w.ground_under(mid);
    let board = lay(&mut w, "deck board", over + up * 0.178, north, up);
    wait(&mut w, ada, 90);
    let tops = rows.map(|(at, _)| {
        let c = centre(&w, board);
        c + (at - c).reject_from(up).dot(north) * north + up * 0.025
    });
    Deck {
        w,
        ada,
        stringers,
        board,
        tops,
        up,
        east,
    }
}

fn joints(w: &ServerWorld, id: EntityId) -> Vec<usize> {
    let s = &w.world.bodies[w.index(id).expect("there")].structure;
    (0..s.element_count())
        .filter(|&e| s.joint_ends(e).is_some())
        .collect()
}

/// J aimed at the stringer's side just under the board (no fastener named): the nail goes as a
/// carpenter drives it, down through the board into the stringer, not up through 150 mm of
/// stringer into 25 mm of board (which held nothing against pulling: ADR-0039's table).
#[test]
fn aimed_at_the_stringer_under_a_board_the_nail_goes_down_through_the_board() {
    let mut d = deck();
    let side = d.tops[0] - d.up * 0.06 - d.east * 0.0375;
    fasten(&mut d.w, d.ada, d.stringers[0], side, "").expect("begins");
    wait(&mut d.w, d.ada, 200);
    let notes = d.w.take_notes();
    assert!(d.w.index(d.board).is_none(), "made one: {notes:?}");
    let js = joints(&d.w, d.stringers[0]);
    assert_eq!(js.len(), 1, "{notes:?}");
    let s = &d.w.world.bodies[d.w.index(d.stringers[0]).expect("built")].structure;
    let withdrawal = s.joint_withdrawal(js[0]).expect("a joint");
    // A 100 mm nail (the shortest at hand that grips fully: 12.5 diameters into the stringer)
    // through 50 mm of board: 20·10⁻⁶·420²·4·50 = 706 N (C24 spruce, 420 kg/m³); up through the
    // stringer none reaches into the board.
    println!("{notes:?}: holds {withdrawal:.0} N against pulling");
    assert!((withdrawal - 705.6).abs() < 3.0, "{withdrawal}");
}

/// The board nailed to the first stringer, then J at its crossing with the second: nailed there
/// too (before, "nothing touches it there": the two are one structure). Then a second nail
/// beside the first at that crossing: one joint of two, holding twice as much, turning stiffer.
#[test]
fn a_board_nailed_to_one_stringer_is_nailed_to_the_next_and_a_crossing_takes_two_nails() {
    let mut d = deck();
    fasten(&mut d.w, d.ada, d.board, d.tops[0], "").expect("begins");
    wait(&mut d.w, d.ada, 200);
    assert!(
        d.w.index(d.stringers[0]).is_none(),
        "{:?}",
        d.w.take_notes()
    );
    fasten(&mut d.w, d.ada, d.board, d.tops[1], "").expect("the next crossing");
    wait(&mut d.w, d.ada, 200);
    let notes = d.w.take_notes();
    assert!(d.w.index(d.stringers[1]).is_none(), "{notes:?}");
    let js = joints(&d.w, d.board);
    assert_eq!(js.len(), 2, "a joint at each crossing: {notes:?}");
    let s = &d.w.world.bodies[d.w.index(d.board).expect("built")].structure;
    let (w1, t1) = (s.joint_withdrawal(js[1]), s.joint_twist(js[1]));
    // The second nail 4 cm along the stringer from the first.
    fasten(&mut d.w, d.ada, d.board, d.tops[1] + d.east * 0.04, "").expect("a second nail");
    wait(&mut d.w, d.ada, 200);
    let notes = d.w.take_notes();
    let js2 = joints(&d.w, d.board);
    assert_eq!(js2, js, "still a joint at each crossing: {notes:?}");
    let s = &d.w.world.bodies[d.w.index(d.board).expect("built")].structure;
    let (w2, t2) = (s.joint_withdrawal(js[1]), s.joint_twist(js[1]));
    println!("{notes:?}: one nail {w1:?} N, {t1:?} N·m/rad; two {w2:?} N, {t2:?} N·m/rad");
    let (w1, w2) = (w1.expect("joint"), w2.expect("joint"));
    // (The second's law is the crossing's as it lies now: settled by a fraction of a millimetre.)
    assert!((w2 / w1 - 2.0).abs() < 0.01, "twice the withdrawal");
    // k·(Σr² + n·d²): from k·(r₁² + d²) to k·(r₁² + r₂² + 2d²), r₂ about 4 cm: many times.
    let (t1, t2) = (t1.expect("joint"), t2.expect("joint"));
    assert!(t2 > 10.0 * t1, "{t2} against {t1}");
}

/// A stringer on edge laid across a deck board on the ground, J aimed at its top with the 100 mm
/// nail chosen (K): it does not come through the stringer's 150 mm into the board, and the player
/// is told so plainly, before any blow (and nothing is made one). (With none chosen, the hand
/// reaches past the nails for a peg, which runs through.)
#[test]
fn a_nail_that_would_not_hold_is_refused_in_plain_words() {
    let mut w = world();
    let ada = w.avatar_of("ada");
    wait(&mut w, ada, 60);
    let me = centre(&w, ada);
    let up = character::plumb(&w.world, me);
    let (east, north) = creeer_terrain::geology::deposits::tangent_frame(up);
    let ground = w.ground_under(me + east * 1.0);
    let board = lay(&mut w, "deck board", ground + up * 0.026, north, up);
    let stringer = lay(&mut w, "stringer", ground + up * 0.128, east, up);
    wait(&mut w, ada, 90);
    let top = centre(&w, stringer) + up * 0.075;
    let refused = fasten(&mut w, ada, stringer, top, "nail_100").expect_err("refused");
    println!("{refused}");
    assert!(
        // (150 mm, or a millimetre more as it lies a little tilted on the board.)
        refused.contains("does not come through the 15"),
        "{refused}"
    );
    wait(&mut w, ada, 200);
    assert!(w.index(board).is_some() && w.index(stringer).is_some());
}
