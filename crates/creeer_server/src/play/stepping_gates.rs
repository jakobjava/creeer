//! Stepping over and onto things, gated (G2.11, A4; ADR-0042, `creeer_sim`'s `legs.rs`): what a
//! person's feet step over the body does not climb, and what it steps up onto it rises onto at a
//! person's pace, pressing it with about its weight.

use super::record::Act;
use super::walking::{Course, Thing, Ticked, course, walk};
use super::walking_loads::{over_the_deck, pushes};
use super::walking_tests::eyes;
use creeer_math::glam::DVec3;
use creeer_net::link::Conditions;

/// Walking east at 1.4 m/s from the spawn over `things` for 9 s: for each, while the body's
/// centre is within a metre of it, the slowest it went (m/s), the eye's highest above the
/// terrain under it (m), and the body's fastest rise (m/s).
fn crossing(things: &[Thing]) -> Vec<(f64, f64, f64)> {
    let mut c: Course = course(things, &["ada"], Conditions::default());
    let (frames, ticks) = walk(&mut c, 9 * 60, |t, c| {
        vec![Act::walk(if t >= 30 { c.east * 1.4 } else { DVec3::ZERO })]
    });
    let (spawn, up) = (c.play.server.world.spawn(), c.up);
    let eyes = eyes(&frames);
    let terrain = &c.play.server.world.terrain;
    let east = |at: DVec3| (at - spawn).dot(c.east);
    things
        .iter()
        .map(|thing| {
            let at = match thing {
                Thing::Log { east, .. } | Thing::Slab { east, .. } => *east,
                _ => unreachable!("logs and slabs"),
            };
            let near: Vec<&Ticked> = (ticks.iter())
                .filter(|t| (east(t.at) - at).abs() < 1.0)
                .collect();
            let slowest = (near.iter())
                .map(|t| t.velocity.reject_from(up).length())
                .fold(f64::MAX, f64::min);
            let rising = (near.iter())
                .map(|t| t.velocity.dot(up))
                .fold(f64::MIN, f64::max);
            let (first, last) = (near[0].tick, near[near.len() - 1].tick);
            let top = (eyes.iter())
                .filter(|(t, _)| (first..=last).contains(t))
                .map(|(_, e)| terrain.density(*e))
                .fold(f64::MIN, f64::max);
            (slowest, top, rising)
        })
        .collect()
}

/// The eye's height standing on level ground (m: `reach::eye`, its legs pressed by its weight).
const EYE: f64 = 1.682;

/// Logs of 10, 20 and 30 cm and a 15 cm sill lying across the way at 1.4 m/s should be stepped
/// over, the eye rising at most 2 cm, never slower than 1.35 m/s. The one ray under the hips
/// climbs each as it passes under them: the eye rises 5, 18, 32 and 14 cm and falls again, the
/// body at up to 1.8 m/s. Five rays along the way, their middle by height, stepped over every
/// one (the eye 0.4–0.7 cm up, 1.40 m/s), but moved the prediction's gate against crates over a
/// slow link (ADR-0042, Measured and rejected).
#[test]
#[ignore = "fails: logs and sills are climbed, not stepped over (ADR-0042, open)"]
fn logs_and_a_sill_are_stepped_over_not_climbed() {
    let things = [
        Thing::Log {
            east: 2.0,
            radius: 0.05,
        },
        Thing::Log {
            east: 4.0,
            radius: 0.1,
        },
        Thing::Log {
            east: 6.0,
            radius: 0.15,
        },
        Thing::Slab {
            east: 8.0,
            size: DVec3::new(0.15, 0.15, 2.4),
        },
    ];
    for (thing, (slowest, top, rising)) in things.iter().zip(crossing(&things)) {
        assert!(top < EYE + 0.02, "{thing:?}: the eye {top:.3} m up");
        assert!(slowest > 1.35, "{thing:?}: slowed to {slowest:.2} m/s");
        assert!(rising < 0.15, "{thing:?}: the body rose at {rising:.2} m/s");
    }
}

/// Onto a slab 20 cm high and 3 m long at 1.4 m/s: the body rises at a person's pace stepping up
/// (at most 0.6 m/s; 0.42 measured), the eye at most 4 cm over standing on it (2.6 cm). Before
/// G2.11 it rose at 1.57 m/s and 6 cm over.
#[test]
fn a_step_up_is_climbed_at_a_persons_pace() {
    let slab = [Thing::Slab {
        east: 2.0,
        size: DVec3::new(3.0, 0.2, 2.4),
    }];
    let (_, top, rising) = crossing(&slab)[0];
    assert!(rising < 0.6, "rose at {rising:.2} m/s");
    assert!(top < EYE + 0.2 + 0.04, "the eye {top:.3} m up");
}

/// Onto a nailed deck 20 cm up, along it, standing on it: the legs press it with a person's
/// weight, at most 1.7 body weights stepping on (1.51 measured; people's footfalls 1.1–1.5),
/// 1.3 walking along it (1.04) and its weight standing. Before G2.11 they pressed it with 2.79
/// stepping on (the legs' whole 3,600 N as the ray found its top), and in *bridge_demo* the first
/// person stepping on pulled out its nails and broke it (A6's measurement).
#[test]
fn the_legs_press_a_deck_with_a_persons_weight() {
    let (c, ticks) = over_the_deck();
    let spawn = c.play.server.world.spawn();
    let east = |t: &Ticked| (t.at - spawn).dot(c.east);
    let on = ticks
        .iter()
        .find(|t| east(t) > 1.6)
        .expect("on the deck")
        .tick;
    let pushes = pushes(&ticks, c.up);
    let most = |range: std::ops::Range<u32>| {
        (pushes.iter())
            .filter(|(t, _)| range.contains(t))
            .map(|p| p.1)
            .fold(f64::MIN, f64::max)
    };
    assert!(
        most(on..on + 30) < 1.7,
        "stepping on: {:.2}",
        most(on..on + 30)
    );
    assert!(
        most(on + 30..200) < 1.3,
        "walking: {:.2}",
        most(on + 30..200)
    );
    assert!(
        (most(230..320) - 1.0).abs() < 0.05,
        "standing: {:.2}",
        most(230..320)
    );
    assert!(ticks.iter().all(|t| !t.fallen), "down");
}
