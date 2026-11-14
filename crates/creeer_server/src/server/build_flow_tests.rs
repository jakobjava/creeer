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
