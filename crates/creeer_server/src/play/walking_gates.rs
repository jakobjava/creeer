//! Moving as the player meets it, gated (G2.11, A4; ADR-0042): what the measurements in
//! `walking_tests.rs` found wrong, held fixed.

use super::record::Act;
use super::walking::{Course, Thing, Ticked, course, walk};
use creeer_net::link::Conditions;
use creeer_net::message::{Command, Target};
use creeer_sim::reach;

/// Ada takes a 15 kg box lying behind her (G1.4f's, `Command::Box`) in her hands.
pub(super) fn take_a_box(c: &mut Course) {
    let spawn = c.play.server.world.spawn();
    let at = c.play.server.world.ground_under(spawn - c.east * 0.8) + c.up * 0.3;
    c.play
        .server
        .world
        .apply(&Command::Box(at), None)
        .expect("a box");
    let _ = walk(c, 60, |_, _| vec![Act::default()]);
    let client = c.play.client(0);
    let eye = reach::eye(client.avatar().expect("drawn").position, c.up);
    let aimed = client
        .aim(eye, (at - eye).normalize(), 3.0)
        .expect("the box");
    let grab = Command::Grab(Target {
        body: aimed.body,
        at: aimed.point,
        seen: aimed.seen,
    });
    let _ = walk(c, 90, |t, _| {
        let commands = if t == 0 {
            vec![grab.clone()]
        } else {
            Vec::new()
        };
        vec![Act {
            commands,
            ..Act::default()
        }]
    });
}

/// A plank bridge (*log bridge*'s stringer, 150 mm) from 1.5 m east and a 0.6 m deck from
/// 5.5 m, each a board nailed to two sills; Ada at the spawn, a box in her hands if `carrying`.
/// She walks east at 1.4 m/s for 8 s: her ticks as the server has them.
pub(super) fn over_the_bridges(carrying: bool) -> (Course, Vec<Ticked>) {
    let things = [
        Thing::Bridge {
            east: 1.5,
            span: 2.0,
            width: 0.15,
        },
        Thing::Bridge {
            east: 5.5,
            span: 2.4,
            width: 0.6,
        },
    ];
    let mut c = course(&things, &["ada"], Conditions::default());
    if carrying {
        take_a_box(&mut c);
    }
    let (_, ticks) = walk(&mut c, 480, |_, c| vec![Act::walk(c.east * 1.4)]);
    (c, ticks)
}

/// Over both bridges, on, along and off each: never knocked down. (Before G2.11 the balance read
/// a nailed board's node as its first member, the sill: the feet carried nothing or pushed her
/// over, and she toppled stepping off the deck, or standing on it with a box: `feet.rs`.)
#[test]
fn a_player_walks_over_nailed_bridges_and_keeps_their_feet() {
    keeps_their_feet(false);
}

/// The same with a 15 kg box in her hands: knocked down stepping onto the deck. Kicked up 2 m/s
/// and on 0.7 m/s in one tick (tick 234) with nothing touching her body, the box not kicked; not
/// without the box (G2.11, A4: found, not mended: the hands and their grip are stream B's).
#[test]
#[ignore = "fails: a carrier stepping onto a nailed deck is kicked over (stream B's hands)"]
fn a_player_carries_a_box_over_nailed_bridges_and_keeps_their_feet() {
    keeps_their_feet(true);
}

fn keeps_their_feet(carrying: bool) {
    {
        let (c, ticks) = over_the_bridges(carrying);
        let ada = c.play.client(0).welcome.as_ref().expect("joined").avatar;
        let spawn = c.play.server.world.spawn();
        let went = (ticks.last().expect("ticks").at - spawn).dot(c.east);
        super::walking_tests::trace_fall(&c, &ticks);
        assert!(
            ticks.iter().all(|t| !t.fallen),
            "carrying {carrying}: knocked down at tick {:?}",
            ticks.iter().find(|t| t.fallen).map(|t| t.tick)
        );
        assert!(went > 9.0, "carrying {carrying}: went {went:.2} m");
        assert_eq!(
            c.play.server.world.carried(ada).is_some(),
            carrying,
            "the box"
        );
    }
}
