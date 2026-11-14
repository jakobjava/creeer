//! Building as the player builds, in first person (G2.10, the user's design, 2026-10-30): a
//! part taken, held where the crosshair meets something within reach and resting against it
//! (`Connection::surface_aimed`, `Connection::place_hold`), stood up (R) or lying, slid along
//! itself (the wheel), set (P) or fastened (J) where it touches: two posts set in their holes
//! and a beam nailed across their tops; a pole laid across two others and fastened.

use super::builder::{Builder, pole, sill};
use creeer_math::glam::DVec3;
use creeer_net::entity::EntityId;
use creeer_net::message::Command;
use std::f64::consts::FRAC_PI_2;

#[test]
fn two_posts_set_in_their_holes_and_a_beam_nailed_across_their_tops() {
    let mut b = Builder::new();
    let holes = [b.ground(1.3, 0.7), b.ground(1.3, -0.7)];
    for mouth in holes {
        b.dig(mouth, 0.6);
    }
    let east = b.east;
    let posts = [
        b.lay(pole(1.6, 0.05), b.ground(0.0, -1.2), east),
        b.lay(pole(1.6, 0.05), b.ground(0.6, -1.4), east),
    ];
    // A 60 mm beam: a 100 mm nail through it reaches 40 mm into a post (24 mm needed).
    let beam = b.lay(pole(1.6, 0.03), b.ground(-0.6, -1.4), east);
    b.game.run(60, super::still);
    // Each post: taken, stood up (R), aimed at its hole's mouth (let down to the floor), set (P)
    // while held: the hands keep it still through the backfilling, then let go.
    let standing = (0.0, FRAC_PI_2, 0.0);
    for (post, mouth) in posts.into_iter().zip(holes) {
        b.take(post);
        b.hold_at(mouth, standing, 150);
        let w = &b.game.server.world;
        let q = w.world.bodies[w.index(post).expect("there")]
            .structure
            .orientation[0];
        let upright = (q * DVec3::X).dot(b.up).abs();
        println!(
            "post held in its hole: {:.1}° from upright",
            creeer_math::det::acos(upright).to_degrees()
        );
        assert!(upright > 0.95, "it stands: {upright}");
        let deep = (mouth - (b.centre(post) - b.up * 0.8)).dot(b.up);
        assert!(deep > 0.3, "its foot {deep:.2} m down");
        b.command(post, mouth, Command::SetPost);
        b.hold_at(mouth, standing, 9 * 60);
        assert!(
            b.game.server.world.carried(b.ada).is_none(),
            "set, let go: {:?}",
            b.told()
        );
    }
    // The beam: taken, aimed at the first post's top, squared to the frame as the view moves (Q)
    // and slid along
    // itself to rest on both; one J nails it to both.
    // Each post's top as seen: its rounded end's crown (aimed at its base, or at the crown
    // itself from a metre off, the crosshair met its near shoulder and the beam rested there,
    // 7 cm off the posts' middles, and slid off it while nailed).
    let crown = |p: EntityId| {
        let top = b.centre(p) + b.up * 0.847;
        let away = (top - b.game.avatar(0)).reject_from(b.up).normalize();
        top + away * 0.02
    };
    let tops = posts.map(crown);
    // Stepped up to the frame (holding the posts through their backfilling, she slid back about
    // 0.3 m each: out of reach of the beam's middle; docs/future-improvements.md).
    b.step_towards(0.5 * (tops[0] + tops[1]), 1.2);
    b.take(beam);
    b.hold_along(tops[0], tops[1], 180);
    // One J, one nail where it rests on the first post (G2.11); the hands keep it still where
    // it is, and a second J nails it to the other where the crosshair is on it.
    b.nail_at(tops[0] + b.up * 0.03, &[beam, posts[1]]);
    b.hold_along(tops[0], tops[1], 3 * 60);
    b.nail_at(tops[1] + b.up * 0.03, &[beam]);
    b.game.run(3 * 60, super::still);
    // Nailed at both ends, let go: it must stand by itself.
    b.game.clients[0].0.command(Command::Drop);
    b.game.run(30, super::still);
    assert!(b.game.server.world.carried(b.ada).is_none(), "let go");
    let parts = [posts[0], posts[1], beam];
    assert_eq!(b.bodies_left(&parts), 1, "one frame: {:?}", b.told());
    // It stands: its highest point (the beam) where it was nailed, three seconds on.
    let frame = *(parts.iter())
        .find(|p| b.game.server.world.index(**p).is_some())
        .expect("the frame");
    let top = |b: &Builder| {
        let w = &b.game.server.world;
        let s = &w.world.bodies[w.index(frame).expect("there")].structure;
        (0..s.node_count())
            .map(|n| s.position[n].dot(b.up))
            .fold(f64::MIN, f64::max)
    };
    let before = top(&b);
    b.game.run(180, super::still);
    let sank = before - top(&b);
    assert!(sank < 0.02, "sank {sank} m");
}

/// A log bridge's deck begun: two squared sills laid along the way across (stringers; round logs
/// roll as the deck is nailed to the first, as the log bridge scene squares its sills) and a pole
/// laid over both, squared to them, set down on them and nailed to each by one J (G2's demo, the
/// user's choice, 2026-10-30).
#[test]
fn a_pole_laid_across_two_logs_and_nailed_to_both() {
    let mut b = Builder::new();
    let east = b.east;
    let stringers = [
        b.lay(sill(), b.ground(1.3, 0.6), east),
        b.lay(sill(), b.ground(1.3, -0.6), east),
    ];
    // A 60 mm pole: a 100 mm nail through it reaches 40 mm into the sill (24 mm needed).
    let deck = b.lay(pole(1.6, 0.03), b.ground(-0.4, -1.4), east);
    b.game.run(90, super::still);
    let tops = stringers.map(|s| b.centre(s) + b.up * 0.075);
    b.take(deck);
    b.hold_along(tops[0], tops[1], 180);
    // One J, one nail; then the other crossing.
    b.nail_at(tops[0] + b.up * 0.04, &[deck, stringers[0]]);
    b.hold_along(tops[0], tops[1], 3 * 60);
    b.nail_at(tops[1] + b.up * 0.04, &[deck]);
    b.game.run(3 * 60, super::still);
    // Let go (G): the nails driven hold it.
    b.game.clients[0].0.command(Command::Drop);
    b.game.run(30, super::still);
    let parts = [stringers[0], stringers[1], deck];
    assert_eq!(b.bodies_left(&parts), 1, "one deck: {:?}", b.told());
}

/// Kit timber of `size` (length, height, width as it lies), C24 spruce.
fn timber(size: DVec3) -> creeer_sim::items::Item {
    use creeer_sim::items::{Form, Item, Piece};
    Item::Piece(
        Piece::solid("spruce_c24", 420.0, Form::Block { size }),
        Vec::new(),
    )
}

/// G2.11 B2: a deck board held across two stringers on edge and nailed where it crosses the
/// first is one with it at the nail's last blow, still in the hands and still where it lay (the
/// nails were joints only once let go, and nothing held it to the first between nails); nailed at
/// the second too, then let go, it stays where it was nailed.
#[test]
fn a_board_nailed_while_held_holds_at_its_last_blow() {
    let mut b = Builder::new();
    let (east, north, up) = (b.east, b.north, b.up);
    let stringers = [-0.2, 0.2].map(|n| {
        let at = b.ground(1.3, n);
        b.lay(timber(DVec3::new(3.0, 0.15, 0.075)), at, east)
    });
    let board = b.lay(
        timber(DVec3::new(1.125, 0.05, 0.15)),
        b.ground(0.0, -1.2),
        north,
    );
    b.game.run(90, super::still);
    let tops = stringers.map(|s| b.centre(s) + up * 0.075);
    b.take(board);
    b.hold_along(tops[0], tops[1], 180);
    b.nail_at(tops[0] + up * 0.05, &[board, stringers[0]]);
    // Its blows (a 100 mm nail through 50 mm of C24: four, 2 s), then held on.
    b.hold_along(tops[0], tops[1], 150);
    let at_first = b.centre(board);
    assert_eq!(
        b.bodies_left(&stringers),
        1,
        "one with the first: {:?}",
        b.told()
    );
    assert_eq!(b.game.server.world.carried(b.ada), Some(board), "held on");
    b.nail_at(tops[1] + up * 0.05, &[board]);
    b.hold_along(tops[0], tops[1], 150);
    assert_eq!(
        b.bodies_left(&stringers),
        0,
        "one with both: {:?}",
        b.told()
    );
    assert_eq!(b.game.server.world.carried(b.ada), Some(board), "held on");
    let held_still = (b.centre(board) - at_first).length();
    b.game.clients[0].0.command(Command::Drop);
    b.game.run(120, super::still);
    let w = &b.game.server.world;
    let s = &w.world.bodies[w.index(board).expect("the deck")].structure;
    let joints = (0..s.element_count())
        .filter(|&e| s.joint_ends(e).is_some())
        .count();
    let moved = (b.centre(board) - at_first).length();
    println!("held still to {held_still:.4} m while nailed; {moved:.4} m off after let go");
    assert_eq!(joints, 2, "{:?}", b.told());
    assert!(held_still < 0.01 && moved < 0.02, "{held_still} {moved}");
}

/// G2.11 B2: a stringer lying flat (its 150 mm across, 75 mm up) is taken, rolled a quarter turn
/// about its length (Z, as `holding.rs` asks it of the hands) and laid on the ground before her:
/// it is held, and lies, on its edge (a stringer flat carries a third of what it carries on edge).
#[test]
fn a_stringer_lying_flat_is_rolled_onto_its_edge() {
    let mut b = Builder::new();
    let (east, north, up) = (b.east, b.north, b.up);
    let stringer = b.lay(
        timber(DVec3::new(3.0, 0.15, 0.075)),
        b.ground(0.0, -1.2),
        east,
    );
    {
        let w = &mut b.game.server.world;
        let i = w.index(stringer).expect("there");
        let body = &mut w.world.bodies[i];
        body.structure.orientation[0] = creeer_math::glam::DQuat::from_mat3(
            &creeer_math::glam::DMat3::from_cols(east, north, east.cross(north)),
        );
        body.solver.reset_history(&body.structure);
    }
    b.game.run(90, super::still);
    let deep = |b: &Builder| {
        let w = &b.game.server.world;
        let q = w.world.bodies[w.index(stringer).expect("there")]
            .structure
            .orientation[0];
        (q * DVec3::Y).dot(up).abs()
    };
    assert!(deep(&b) < 0.1, "flat to begin with: {}", deep(&b));
    b.take(stringer);
    let client = &b.game.clients[0].0;
    let drawn = client.poses()[&stringer].orientation;
    b.asked = client.rolled(b.asked.unwrap_or(drawn), std::f64::consts::FRAC_PI_2);
    let ahead = b.ground(1.3, 0.0);
    b.hold_at(ahead, (0.0, 0.0, 0.0), 120);
    let held = deep(&b);
    b.game.clients[0].0.command(Command::Drop);
    b.game.run(120, super::still);
    println!("on edge held {held:.3}, let go {:.3} (cosines)", deep(&b));
    assert!(held > 0.97 && deep(&b) > 0.97, "{held} {}", deep(&b));
}

/// G2.11 B2 (the deck's frames): a 3 m stringer taken lying across the view and carried before
/// the chest is turned along the view (Q). The hands' plan turned it at 8 rad/s whatever it
/// weighed, and it swung past and back (90° to 23°, out to 62° the other way, for seconds);
/// planned as their torque turns and stops it, it comes within 5° in four seconds.
#[test]
fn a_stringer_taken_across_the_view_is_turned_along_it() {
    let mut b = Builder::new();
    let (east, north, up) = (b.east, b.north, b.up);
    let item = timber(DVec3::new(3.0, 0.15, 0.075));
    let stringer = b.lay(item, b.ground(1.2, 0.0), north);
    b.game.run(90, super::still);
    b.take(stringer);
    let level = super::builder::looking(east, up);
    let along = |b: &Builder| {
        let w = &b.game.server.world;
        let q = w.world.bodies[w.index(stringer).expect("there")]
            .structure
            .orientation[0];
        creeer_math::det::acos((q * DVec3::X).dot(east).abs().min(1.0)).to_degrees()
    };
    for tick in 0..240u32 {
        let client = &mut b.game.clients[0].0;
        let hold = client.place_hold(
            level,
            None,
            (std::f64::consts::FRAC_PI_2, 0.0, 0.0),
            b.asked,
        );
        b.asked = hold.map(|h| h.turn);
        client.set_hold(hold);
        b.game.tick(super::still);
        if tick % 20 == 0 {
            println!("tick {tick}: {:.1}° off the view", along(&b));
        }
    }
    assert!(along(&b) < 5.0, "{:.1}° off the view", along(&b));
}

/// The user's pole (G2.9: "how to dig it into the ground?"; 3 m of green spruce, 12 cm: 27 kg):
/// taken at its middle, stood up (R), aimed at its hole's mouth and let down to its floor, set
/// (P) while held, it stands. (Raised by its end and walked under, as before the first-person
/// placing, `raising_tests.rs`.)
#[test]
fn a_heavy_pole_stood_up_and_set_in_its_hole() {
    let mut b = Builder::new();
    let mouth = b.ground(1.4, 0.0);
    b.dig(mouth, 0.7);
    let east = b.east;
    let pole = b.lay(pole(3.0, 0.06), b.ground(0.0, -1.3), east);
    b.game.run(60, super::still);
    b.take(pole);
    let standing = (0.0, FRAC_PI_2, 0.0);
    b.hold_at(mouth, standing, 180);
    let w = &b.game.server.world;
    let q = w.world.bodies[w.index(pole).expect("there")]
        .structure
        .orientation[0];
    let upright = (q * DVec3::X).dot(b.up).abs();
    println!(
        "pole held in its hole: {:.1}° from upright",
        creeer_math::det::acos(upright).to_degrees()
    );
    assert!(upright > 0.95, "it stands: {upright}");
    let deep = (mouth - (b.centre(pole) - b.up * 1.5)).dot(b.up);
    assert!(deep > 0.6, "its foot {deep:.2} m down");
    b.command(pole, mouth, Command::SetPost);
    b.hold_at(mouth, standing, 9 * 60);
    assert!(
        b.game.server.world.carried(b.ada).is_none(),
        "set, let go: {:?}",
        b.told()
    );
    b.game.run(180, super::still);
    let w = &b.game.server.world;
    let s = &w.world.bodies[w.index(pole).expect("there")].structure;
    // (Set, it is a structure: its axis from its nodes, lowest to highest.)
    let along = |n: usize| s.position[n].dot(b.up);
    let (low, high) = (0..s.node_count()).fold((0, 0), |(l, h), n| {
        (
            if along(n) < along(l) { n } else { l },
            if along(n) > along(h) { n } else { h },
        )
    });
    let upright = (s.position[high] - s.position[low]).normalize().dot(b.up);
    assert!(upright > 0.95, "set, it stands: {upright}");
}
