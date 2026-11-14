//! Moving and looking measured as the player meets them (G2.11, A4; ADR-0042): walking,
//! running, stopping and turning on level ground, the eye drawn frame by frame, over a lossless
//! and a slow link.

use super::record::Act;
use super::walking::{Course, Frame, Thing, Ticked, course, slow, walk};
use creeer_math::glam::DVec3;
use creeer_net::link::Conditions;
use creeer_sim::reach;

/// The ground speed of a tick (m/s).
fn speed(t: &Ticked, up: DVec3) -> f64 {
    t.velocity.reject_from(up).length()
}

/// From tick `from`: the ticks until the speed along `way` first reaches `share` of `target`, and
/// how far the body went along the ground by then (m).
pub(super) fn reaching(
    ticks: &[Ticked],
    (from, up): (u32, DVec3),
    (way, target, share): (DVec3, f64, f64),
) -> Option<(u32, f64)> {
    let start = ticks.iter().find(|t| t.tick == from)?.at;
    let reached = ticks
        .iter()
        .filter(|t| t.tick >= from)
        .find(|t| t.velocity.dot(way) >= share * target)?;
    Some((
        reached.tick - from,
        (reached.at - start).reject_from(up).length(),
    ))
}

/// From tick `from`: the ticks until the body stands (slower than 5 cm/s along the ground), and
/// how far it went meanwhile (m).
pub(super) fn stopping(ticks: &[Ticked], (from, up): (u32, DVec3)) -> Option<(u32, f64)> {
    let start = ticks.iter().find(|t| t.tick == from)?.at;
    let stood = ticks
        .iter()
        .filter(|t| t.tick > from)
        .find(|t| speed(t, up) < 0.05)?;
    Some((
        stood.tick - from,
        (stood.at - start).reject_from(up).length(),
    ))
}

/// The first-person eye of each frame, as the game puts its camera (`planet_view.rs`, `follow`).
pub(super) fn eyes(frames: &[Frame]) -> Vec<(u32, DVec3)> {
    frames
        .iter()
        .map(|f| {
            let p = f.drawn.position;
            (f.tick, reach::eye(p, p.normalize()))
        })
        .collect()
}

/// The eye's jerks over `ticks`: each frame's second difference (m: how far it is off the
/// straight line between the frames either side), up and along the ground, the most and the
/// 99th percentile of each (mm).
pub(super) fn jerks(
    eyes: &[(u32, DVec3)],
    ticks: std::ops::Range<u32>,
    up: DVec3,
) -> [(f64, f64); 2] {
    let (mut vertical, mut level) = (Vec::new(), Vec::new());
    for w in eyes.windows(3).filter(|w| ticks.contains(&w[1].0)) {
        let d = w[2].1 - 2.0 * w[1].1 + w[0].1;
        vertical.push(d.dot(up).abs() * 1e3);
        level.push(d.reject_from(up).length() * 1e3);
    }
    [vertical, level].map(|mut v| {
        v.sort_by(f64::total_cmp);
        let most = v.last().copied().unwrap_or(0.0);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let p99 = v
            .get(((v.len() as f64 - 1.0) * 0.99).round() as usize)
            .copied()
            .unwrap_or(0.0);
        (most, p99)
    })
}

/// The eye's height above the ground under it over `ticks`: least and most (m).
pub(super) fn heights(
    course: &Course,
    eyes: &[(u32, DVec3)],
    ticks: std::ops::Range<u32>,
) -> (f64, f64) {
    let terrain = &course.play.server.world.terrain;
    eyes.iter()
        .filter(|(t, _)| ticks.contains(t))
        .map(|(_, e)| terrain.density(*e))
        .fold((f64::MAX, f64::MIN), |(lo, hi), h| (lo.min(h), hi.max(h)))
}

/// The level script's parts (ticks): walking as the game walks (3 m/s), stopped; running as it
/// runs (6.5 asked, 4 the most), stopped; walking at a person's 1.4 m/s, turning a right angle,
/// then about; standing, a jump.
const WALK: std::ops::Range<u32> = 60..240;
const RUN: std::ops::Range<u32> = 360..540;
const STROLL: std::ops::Range<u32> = 660..780;
const TURN: std::ops::Range<u32> = 780..900;
const ABOUT: std::ops::Range<u32> = 900..1020;
const JUMP: u32 = 1100;
const LEVEL_TICKS: u32 = 1200;

fn level_script(t: u32, c: &Course) -> Vec<Act> {
    let mut act = match t {
        t if WALK.contains(&t) => Act::walk(c.east * 3.0),
        t if RUN.contains(&t) => Act::walk(c.east * 6.5),
        t if STROLL.contains(&t) => Act::walk(c.east * 1.4),
        t if TURN.contains(&t) => Act::walk(c.north * 1.4),
        t if ABOUT.contains(&t) => Act::walk(-c.north * 1.4),
        _ => Act::default(),
    };
    act.jump = t == JUMP;
    vec![act]
}

fn measure_level(name: &str, link: Conditions) {
    let mut c = course(&[], &["ada"], link);
    let (frames, ticks) = walk(&mut c, LEVEL_TICKS, level_script);
    let up = c.up;
    let at = |from| (from, up);
    eprintln!("== level, {name}");
    eprintln!(
        "walk 3.0 asked: to 0.1 m/s {:?}, to 90 % {:?} (ticks, m)",
        reaching(&ticks, at(WALK.start), (c.east, 3.0, 0.033)),
        reaching(&ticks, at(WALK.start), (c.east, 3.0, 0.9))
    );
    eprintln!(
        "  steady {:.2} m/s; stop {:?}",
        speed(&ticks[(WALK.end - 1) as usize], up),
        stopping(&ticks, at(WALK.end))
    );
    eprintln!(
        "run 6.5 asked: to 90 % of 4 {:?}; steady {:.2} m/s; stop {:?}",
        reaching(&ticks, at(RUN.start), (c.east, 4.0, 0.9)),
        speed(&ticks[(RUN.end - 1) as usize], up),
        stopping(&ticks, at(RUN.end))
    );
    eprintln!(
        "stroll 1.4: to 90 % {:?}; turn 90°: along the new way to 90 % {:?}; about: {:?}",
        reaching(&ticks, at(STROLL.start), (c.east, 1.4, 0.9)),
        reaching(&ticks, at(TURN.start), (c.north, 1.4, 0.9)),
        reaching(&ticks, at(ABOUT.start), (-c.north, 1.4, 0.9)),
    );
    let eyes = eyes(&frames);
    for (part, range) in [
        ("standing", 20..60),
        ("walking 3", WALK.start + 60..WALK.end),
        ("running 4", RUN.start + 60..RUN.end),
        ("strolling 1.4", STROLL.start + 40..STROLL.end),
        ("turning", TURN),
        ("stopping", WALK.end..WALK.end + 60),
        ("jump", JUMP..JUMP + 100),
    ] {
        let [v, h] = jerks(&eyes, range.clone(), up);
        let (lo, hi) = heights(&c, &eyes, range);
        eprintln!(
            "  eye {part}: jerk up most {:.2} p99 {:.2} mm, level most {:.2} p99 {:.2} mm; \
             height {lo:.3}–{hi:.3} m",
            v.0, v.1, h.0, h.1
        );
    }
    let k = c.play.client(0).corrections();
    eprintln!(
        "  corrections {} over 1 cm {} largest {:.4} m",
        k.count, k.over_a_centimetre, k.largest
    );
}

#[test]
#[ignore = "measures moving and looking over the real client: by hand"]
fn measure_moving_on_level_ground() {
    measure_level("lossless", Conditions::default());
    measure_level("slow", slow());
}

/// The step script: walking east at `speed` over the course's things.
fn steps_script(speed: f64) -> impl Fn(u32, &Course) -> Vec<Act> {
    move |t, c| {
        vec![if (30..30 + 60 * 14).contains(&t) {
            Act::walk(c.east * speed)
        } else {
            Act::default()
        }]
    }
}

fn measure_steps(name: &str, things: &[Thing], speed: f64, link: Conditions) {
    let mut c = course(things, &["ada"], link);
    let (frames, ticks) = walk(&mut c, 60 * 15, steps_script(speed));
    let up = c.up;
    let spawn = c.play.server.world.spawn();
    eprintln!("== {name} at {speed} m/s");
    let eyes = eyes(&frames);
    for (k, thing) in things.iter().enumerate() {
        let east = match thing {
            Thing::Log { east, .. }
            | Thing::Slab { east, .. }
            | Thing::Mound { east, .. }
            | Thing::Bridge { east, .. } => *east,
        };
        // The ticks the body's centre is within a metre of it.
        let near: Vec<&Ticked> = (ticks.iter())
            .filter(|t| ((t.at - spawn).dot(c.east) - east).abs() < 1.0)
            .collect();
        let (Some(first), Some(last)) = (near.first(), near.last()) else {
            eprintln!("  {thing:?}: never reached");
            continue;
        };
        let slowest = near
            .iter()
            .map(|t| speed_of(t, up))
            .fold(f64::MAX, f64::min);
        let rising = near
            .iter()
            .map(|t| t.velocity.dot(up))
            .fold(f64::MIN, f64::max);
        let falling = near
            .iter()
            .map(|t| t.velocity.dot(up))
            .fold(f64::MAX, f64::min);
        let fell = near.iter().any(|t| t.fallen);
        let (_, top) = heights(&c, &eyes, first.tick..last.tick + 1);
        let (_, body_top) = (near.iter())
            .map(|t| c.play.server.world.terrain.density(t.at))
            .fold((0.0, f64::MIN), |(a, b), h| (a, f64::max(b, h)));
        eprintln!(
            "  eye's most above the ground {top:.3} m (1.68 standing), the body's centre {body_top:.3} (1.15)"
        );
        let [v, h] = jerks(&eyes, first.tick..last.tick + 1, up);
        let moved = c.things[k].map(|id| {
            let w = &c.play.server.world;
            w.index(id).map_or(-1.0, |i| {
                (w.world.bodies[i].structure.position[0] - spawn).dot(c.east)
            })
        });
        eprintln!(
            "  {thing:?}: ticks {}–{}, slowest {slowest:.2} m/s, rising {rising:.2} falling \
             {falling:.2} m/s, fell {fell}, eye jerk up {:.1}/{:.1} level {:.1}/{:.1} mm; it \
             lies {moved:?} m east",
            first.tick, last.tick, v.0, v.1, h.0, h.1
        );
    }
    let end = ticks.last().expect("ticks");
    eprintln!(
        "  went {:.2} m east, fell {}",
        (end.at - spawn).dot(c.east),
        ticks.iter().any(|t| t.fallen)
    );
}

fn speed_of(t: &Ticked, up: DVec3) -> f64 {
    speed(t, up)
}

/// Standing on a slope (the player starts on a mound's flank, `east` m from its top on a 10 m
/// ball: asin(east/10) steep) for 6 s, walking up over it and down at 1.4 m/s for 5 s, standing
/// on its far flank 5 s: how far it crept standing each time (m), and its speed up and down.
fn measure_slope(east: f64) {
    let mound = Thing::Mound {
        east,
        radius: 10.0,
        high: 2.0,
    };
    let mut c = course(&[mound], &["ada"], Conditions::default());
    let script = |t: u32, c: &Course| {
        vec![if (360..660).contains(&t) {
            Act::walk(c.east * 1.4)
        } else {
            Act::default()
        }]
    };
    let (_, ticks) = walk(&mut c, 960, script);
    let up = c.up;
    let crept = |a: u32, b: u32| {
        (ticks[b as usize].at - ticks[a as usize].at)
            .reject_from(up)
            .length()
    };
    let speeds: Vec<f64> = (380..660)
        .step_by(40)
        .map(|t| speed(&ticks[t], up))
        .collect();
    eprintln!(
        "slope {:.1}°: standing crept {:.4} m in 5 s; walking {speeds:.2?} m/s; standing on the far \
         side crept {:.4} m in 4 s; fell {}",
        creeer_math::det::asin(east / 10.0).to_degrees(),
        crept(60, 360),
        crept(720, 959),
        ticks.iter().any(|t| t.fallen)
    );
}

#[test]
#[ignore = "measures standing and walking on slopes over the real client: by hand"]
fn measure_slopes() {
    for east in [2.0, 3.4, 5.0] {
        measure_slope(east);
    }
}

/// The ticks about the first in which the player fell, printed.
pub(super) fn trace_fall(c: &Course, ticks: &[Ticked]) {
    let Some(fell) = ticks.iter().position(|t| t.fallen) else {
        return;
    };
    let spawn = c.play.server.world.spawn();
    let terrain = &c.play.server.world.terrain;
    for t in &ticks[fell.saturating_sub(12)..(fell + 3).min(ticks.len())] {
        eprintln!(
            "  tick {}: lean {:.3}, {:.2} m east, {:+.3} north, centre {:.3} over the ground, v \
             {:+.2} up {:+.2} east {:+.2} north, fallen {}",
            t.tick,
            t.lean,
            (t.at - spawn).dot(c.east),
            (t.at - spawn).dot(c.north),
            terrain.density(t.at),
            t.velocity.dot(c.up),
            t.velocity.dot(c.east),
            t.velocity.dot(c.north),
            t.fallen
        );
    }
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

#[test]
#[ignore = "measures stepping over things over the real client: by hand"]
fn measure_stepping() {
    let logs = [
        Thing::Log {
            east: 3.0,
            radius: 0.05,
        },
        Thing::Log {
            east: 7.0,
            radius: 0.1,
        },
        Thing::Log {
            east: 11.0,
            radius: 0.15,
        },
    ];
    let sill_and_deck = [
        Thing::Slab {
            east: 3.0,
            size: DVec3::new(0.15, 0.15, 2.4),
        },
        Thing::Slab {
            east: 7.0,
            size: DVec3::new(3.0, 0.2, 2.4),
        },
    ];
    for speed in [1.4, 3.0] {
        measure_steps("logs", &logs, speed, Conditions::default());
        measure_steps(
            "sill and deck",
            &sill_and_deck,
            speed,
            Conditions::default(),
        );
    }
}
