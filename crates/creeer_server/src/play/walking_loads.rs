//! What a person puts into what they stand on (G2.11, A4; ADR-0042): the legs' push on a nailed
//! deck as the person steps onto it, walks along it, stands on it and steps off, measured from
//! the body's own acceleration each tick (the legs are all that hold it up), in body weights.
//! People press the ground with about their weight standing, and walking with peaks of about
//! 1.1–1.5 body weights a footfall (the gait literature); a board loaded in one span lifts its
//! far end with a tenth to a fifth of that.

use super::record::Act;
use super::walking::{Course, Thing, Ticked, course, walk};
use super::walking_tests::{eyes, heights, jerks, trace_fall};
use creeer_math::glam::DVec3;
use creeer_net::link::Conditions;

/// The legs' push each tick (body weights): the body's mass times its acceleration up and
/// gravity's, over its weight.
pub(super) fn pushes(ticks: &[Ticked], up: creeer_math::glam::DVec3) -> Vec<(u32, f64)> {
    const G: f64 = 9.806_65;
    ticks
        .windows(2)
        .map(|w| {
            let a = (w[1].velocity - w[0].velocity).dot(up) * 60.0;
            (w[1].tick, (a + G) / G)
        })
        .collect()
}

/// The deck: a 0.6 m board 50 mm thick over two sills 2.4 m apart, nailed to both, from 2 m east
/// of the spawn (its board from 1.7 to 4.7 m).
pub(super) const DECK: Thing = Thing::Bridge {
    east: 2.0,
    span: 2.4,
    width: 0.6,
};

/// Onto the deck at 1.4 m/s, to its middle, standing 2 s, on and off it: each tick's push.
pub(super) fn over_the_deck() -> (Course, Vec<Ticked>) {
    let mut c = course(&[DECK], &["ada"], Conditions::default());
    let spawn = c.play.server.world.spawn();
    let (_, mut ticks) = walk(&mut c, 600, move |t, c| {
        let along = (c.play.avatar(0) - spawn).dot(c.east);
        let stand = (200..320).contains(&t) || (t < 200 && along > 3.2);
        vec![Act::walk(if stand { DVec3::ZERO } else { c.east * 1.4 })]
    });
    ticks.retain(|t| t.tick > 0);
    (c, ticks)
}

#[test]
#[ignore = "measures the legs' push on a nailed deck over the real client: by hand"]
fn measure_the_legs_push_on_a_deck() {
    let (c, ticks) = over_the_deck();
    let spawn = c.play.server.world.spawn();
    let along = |t: &Ticked| (t.at - spawn).dot(c.east);
    let pushes = pushes(&ticks, c.up);
    let mut on = None;
    for (k, (tick, push)) in pushes.iter().enumerate() {
        let t = &ticks[k + 1];
        if on.is_none() && along(t) > 1.6 {
            on = Some(*tick);
        }
        if std::env::var("TRACE").is_ok() && (along(t) > 1.4 && along(t) < 5.2) {
            eprintln!(
                "tick {tick}: {:.2} m east, push {push:.2} body weights, rising {:+.2} m/s",
                along(t),
                t.velocity.dot(c.up)
            );
        }
    }
    let part = |range: std::ops::Range<u32>| {
        let ps: Vec<f64> = (pushes.iter())
            .filter(|(t, _)| range.contains(t))
            .map(|p| p.1)
            .collect();
        #[allow(clippy::cast_precision_loss)]
        let mean = ps.iter().sum::<f64>() / ps.len().max(1) as f64;
        let most = ps.iter().copied().fold(f64::MIN, f64::max);
        let rate = ps
            .windows(2)
            .map(|w| (w[1] - w[0]).abs() * 60.0)
            .fold(0.0, f64::max);
        (most, mean, rate)
    };
    let on = on.unwrap_or(0);
    eprintln!(
        "stepping on (tick {on}): most, mean, fastest change (body weights, /s) {:.2?}",
        part(on..on + 30)
    );
    eprintln!("walking along it: {:.2?}", part(on + 30..200));
    eprintln!("standing on it: {:.2?}", part(230..320));
    eprintln!("walking off it: {:.2?}", part(320..600));
    let w = &c.play.server.world;
    eprintln!(
        "the deck: {:?}; the person down {}",
        c.things[0].and_then(|id| w.index(id)).map(|i| {
            let s = &w.world.bodies[i].structure;
            (s.elements_broken(), s.pieces().len())
        }),
        ticks.iter().any(|t| t.fallen)
    );
}
/// Walking along a plank bridge (*log bridge*'s, 150 mm) and over a 0.6 m deck, standing on the
/// deck's middle for 3 s, then on; with or without a 15 kg box in the hands.
fn measure_bridge(carrying: bool) {
    let things = [
        Thing::Bridge {
            east: 3.0,
            span: 2.0,
            width: 0.15,
        },
        Thing::Bridge {
            east: 8.0,
            span: 2.4,
            width: 0.6,
        },
    ];
    let mut c = course(&things, &["ada"], Conditions::default());
    let spawn = c.play.server.world.spawn();
    let up = c.up;
    let (east, north) = (c.east, c.north);
    if carrying {
        super::walking_gates::take_a_box(&mut c);
    }
    let middle = 9.2;
    let script = move |_t: u32, c: &Course| {
        let along = (c.play.avatar(0) - spawn).dot(east);
        vec![Act::walk(if along < middle {
            east * 1.4
        } else {
            DVec3::ZERO
        })]
    };
    let (frames, ticks) = walk(&mut c, 500, script);
    let (more, after) = walk(&mut c, 240, |_, c| vec![Act::walk(c.east * 1.4)]);
    if std::env::var("TRACE").is_ok() {
        trace_fall(&c, &ticks);
        trace_fall(&c, &after);
    }
    let eyes = eyes(&frames);
    let off = |t: &Ticked| (t.at - spawn).dot(north);
    let sideways = ticks
        .iter()
        .chain(&after)
        .map(|t| off(t).abs())
        .fold(0.0, f64::max);
    let lowest = ticks
        .iter()
        .chain(&after)
        .map(|t| t.velocity.dot(up))
        .fold(0.0, f64::min);
    let highest = ticks
        .iter()
        .chain(&after)
        .map(|t| t.velocity.dot(up))
        .fold(0.0, f64::max);
    let [v, h] = jerks(&eyes, 0..500, up);
    let held = c
        .play
        .server
        .world
        .carried(c.play.client(0).welcome.as_ref().expect("joined").avatar);
    eprintln!(
        "bridges (carrying {carrying}): fell {}, off the line at most {sideways:.3} m, vertical \
         speed {lowest:.2}–{highest:.2} m/s, eye jerk up {:.1}/{:.1} level {:.1}/{:.1} mm, \
         ended {:.2} m east, holding {held:?}",
        ticks.iter().chain(&after).any(|t| t.fallen),
        v.0,
        v.1,
        h.0,
        h.1,
        (after.last().expect("ticks").at - spawn).dot(east),
    );
    // Standing on the deck's middle: how the body moves (sway) and how far it creeps.
    let still: Vec<&Ticked> = ticks.iter().filter(|t| t.tick > 400).collect();
    if let (Some(a), Some(b)) = (still.first(), still.last()) {
        let sway = still
            .iter()
            .map(|t| t.velocity.length())
            .fold(0.0, f64::max);
        eprintln!(
            "  standing on the deck: crept {:.4} m in {} ticks, fastest {sway:.3} m/s, eye {:.3}–{:.3} m over the ground",
            (b.at - a.at).length(),
            b.tick - a.tick,
            heights(&c, &eyes, 400..500).0,
            heights(&c, &eyes, 400..500).1,
        );
    }
    let _ = more;
}

#[test]
#[ignore = "measures walking over bridges over the real client: by hand"]
fn measure_bridges() {
    measure_bridge(false);
    measure_bridge(true);
}
