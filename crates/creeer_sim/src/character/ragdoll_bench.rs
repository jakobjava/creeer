//! (P1, ADR-0024.) A bench of ragdolls in trouble, run by hand: `cargo test -p creeer_sim
//! --release --lib ragdoll_ -- --ignored --nocapture`. Each case a character knocked down by
//! a 200 kg crate dropped on it (from a few places), or a ragdoll toppled alone; ten seconds
//! each.
//! - `ragdoll_bench`: whole runs. Chaotic, so judged as a family: the time in all, the worst
//!   tick, how many lie asleep at the end, and when (how long things move is physics, not
//!   speed: compare builds with the next two).
//! - `ragdoll_rest_bench`: 24 whole runs over a grid of drops, for whether ragdolls come to
//!   rest (seven cases are too few: a change moved the restlessness from case to case).
//! - `ragdoll_states_written` and `ragdoll_states_stepped`: every state after the fall written
//!   once (`RAGDOLL_STATES=<file>`), then stepped once each by any build: the same states for
//!   all.

use super::ragdoll::{fall, knocked};
use super::*;
use creeer_physics::world::shape::StaticShape;

const DT: f64 = 1.0 / 60.0;

/// Where the crate is dropped, from above the character's head (none: no crate, the
/// character toppled).
const CASES: [Option<DVec3>; 7] = [
    Some(DVec3::new(0.15, 0.0, 0.0)),
    Some(DVec3::new(-0.15, 0.0, 0.1)),
    Some(DVec3::new(0.0, 0.0, 0.3)),
    Some(DVec3::new(0.3, 0.5, -0.2)),
    Some(DVec3::new(-0.35, 1.0, 0.0)),
    Some(DVec3::new(0.05, 0.0, -0.35)),
    None,
];

/// One case (`crate_at`, as [`CASES`]): (time in all, worst tick, ticks until all asleep).
/// Each state after the fall goes into `states` (as saved: then there are no struts).
fn case(crate_at: Option<DVec3>, states: &mut Vec<Vec<u8>>) -> (f64, f64, Option<u32>) {
    let mut world = World::new(DT);
    world.statics.push(StaticShape::Plane {
        normal: DVec3::Y,
        offset: 0.0,
    });
    let c = world.add(body(DVec3::ZERO, DVec3::Y, DT));
    let mut reactions = Vec::new();
    let idle = Intent::default();
    for _ in 0..60 {
        steer(&mut world, c, idle, (DVec3::Y, None), &mut reactions);
        world.step();
        release(&mut world, &mut reactions);
    }
    let mut down = false;
    match crate_at {
        Some(offset) => {
            let at = DVec3::new(0.0, HEIGHT + 1.5, 0.0) + offset;
            let mut b = Body::rigid(
                Shape::Box {
                    half: DVec3::splat(0.5),
                },
                200.0,
                at,
                DT,
            );
            b.structure.gravity = world.bodies[c].structure.gravity;
            b.friction = 0.7;
            world.add(b);
        }
        None => {
            world.bodies[c].structure.angular_velocity[0] = DVec3::new(2.0, 0.0, 0.5);
            fall(&mut world, c);
            down = true;
        }
    }
    let (mut total, mut worst, mut slept) = (0.0, 0.0f64, None);
    for t in 0..600 {
        if slept.is_none() && world.bodies.iter().all(|b| b.sleep.asleep) {
            slept = Some(t);
        }
        let before = world.bodies[c].structure.velocity[0];
        if down {
            states.push(world.to_bytes());
        } else {
            steer(&mut world, c, idle, (DVec3::Y, None), &mut reactions);
        }
        let start = std::time::Instant::now();
        world.step();
        let ms = start.elapsed().as_secs_f64() * 1e3;
        let pushed = injury::pushed_on(&world.strut_pushes(), c);
        release(&mut world, &mut reactions);
        if !down && knocked(&world, c, DVec3::Y, (before, pushed)) {
            down = true;
            fall(&mut world, c);
        }
        (total, worst) = (total + ms, worst.max(ms));
    }
    (total, worst, slept)
}

#[test]
#[ignore = "a bench, run by hand"]
fn ragdoll_bench() {
    let (mut total, mut worst, mut asleep) = (0.0, 0.0f64, 0);
    for at in CASES {
        let (t, w, s) = case(at, &mut Vec::new());
        eprintln!("  {at:?}: {t:.0} ms, worst {w:.1} ms, asleep from {s:?}");
        (total, worst) = (total + t, worst.max(w));
        asleep += usize::from(s.is_some());
    }
    eprintln!(
        "ragdoll bench: {total:.0} ms in all, worst tick {worst:.1} ms, {asleep} of 7 asleep"
    );
}

/// A family of 24 cases for judging whether ragdolls come to rest (seven chaotic cases moved
/// their restlessness from one to another with every change): the crate dropped over a grid
/// of places about the head, ten seconds each. The time in all, how many rest (all their
/// bodies asleep) by the end, and the median tick they rest from.
#[test]
#[ignore = "a bench, run by hand"]
fn ragdoll_rest_bench() {
    let (mut total, mut worst) = (0.0, 0.0f64);
    let mut rested = Vec::new();
    for x in [-0.35, -0.15, 0.05, 0.25] {
        for z in [-0.3, 0.0, 0.3] {
            for y in [0.0, 0.8] {
                let (t, w, s) = case(Some(DVec3::new(x, y, z)), &mut Vec::new());
                (total, worst) = (total + t, worst.max(w));
                rested.extend(s);
            }
        }
    }
    rested.sort_unstable();
    let median = rested.get(rested.len() / 2).copied();
    eprintln!(
        "rest bench: {total:.0} ms in all, worst tick {worst:.1} ms, {} of 24 at rest, \
         median from tick {median:?}",
        rested.len()
    );
}

/// The cases' states after the fall, each tick of each, written to `RAGDOLL_STATES` (a file):
/// the same states for every build, stepped by [`ragdoll_states_stepped`].
#[test]
#[ignore = "a bench, run by hand"]
fn ragdoll_states_written() {
    let path = std::env::var("RAGDOLL_STATES").expect("RAGDOLL_STATES=<file>");
    let mut all: Vec<Vec<u8>> = Vec::new();
    for at in CASES {
        case(at, &mut all);
    }
    let mut w = creeer_math::codec::Writer::new();
    w.varint(all.len() as u64);
    for bytes in &all {
        w.varint(bytes.len() as u64);
        w.raw(bytes);
    }
    std::fs::write(path, w.finish()).unwrap();
    eprintln!("{} states written", all.len());
}

/// Each state of `RAGDOLL_STATES` stepped once: the time in all, the worst step, the Newton
/// iterations (each step's most of any body) in all, and the slowest eight (ms, state,
/// iterations). `ONLY=<state>` steps that one alone (with the `profile` feature, its phases).
#[test]
#[ignore = "a bench, run by hand"]
fn ragdoll_states_stepped() {
    let path = std::env::var("RAGDOLL_STATES").expect("RAGDOLL_STATES=<file>");
    let bytes = std::fs::read(path).unwrap();
    let mut r = creeer_math::codec::Reader::new(&bytes);
    let n = usize::try_from(r.varint().unwrap()).unwrap();
    let states: Vec<Vec<u8>> = (0..n)
        .map(|_| r.bytes_with_length().unwrap().to_vec())
        .collect();
    let _ = creeer_physics::profile::take();
    let (mut total, mut worst, mut its) = (0.0, 0.0f64, 0u64);
    let mut slow: Vec<(f64, usize, u32)> = Vec::new();
    let only: Option<usize> = std::env::var("ONLY").ok().map(|s| s.parse().unwrap());
    for (k, state) in states.iter().enumerate() {
        if only.is_some_and(|o| o != k) {
            continue;
        }
        let mut world = World::from_bytes(state).unwrap();
        let start = std::time::Instant::now();
        world.step();
        let ms = start.elapsed().as_secs_f64() * 1e3;
        (total, worst) = (total + ms, worst.max(ms));
        let most = (world.bodies.iter())
            .map(|b| b.solver.last_iterations)
            .max()
            .unwrap_or(0);
        its += u64::from(most);
        slow.push((ms, k, most));
    }
    slow.sort_by(|a, b| b.0.total_cmp(&a.0));
    eprintln!("slowest {:?}", &slow[..slow.len().min(8)]);
    eprintln!("{n} states stepped: {total:.0} ms in all, worst {worst:.2} ms, {its} iterations");
    let spent = creeer_physics::profile::take();
    let phases: Vec<String> = (creeer_physics::profile::NAMES.iter())
        .filter(|(s, _)| spent[*s as usize] > 0)
        .map(|(s, n)| format!("{n} {}", spent[*s as usize]))
        .collect();
    if !phases.is_empty() {
        eprintln!("  phases (ns or counts): {}", phases.join(", "));
    }
}
