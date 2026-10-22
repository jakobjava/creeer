//! The motion as a player sees it (G1.4a): one player through a script of standing, walking,
//! stopping, running, turning and jumping, over the real server and client (`creeer_server::play`),
//! drawn by the animator on the planet's ground, frame by frame. What jumps between frames is a
//! glitch; a stop takes at most a step; running, the trunk leans as runners' do.

use super::{Animator, Body};
use crate::people_ground::PlanetGround;
use creeer_math::Frame64;
use creeer_math::glam::DVec3;
use creeer_server::play::Play;
use creeer_server::play::record::Act;
use creeer_server::play::scenes::{terrain, world};

/// The script's parts (ticks): walking at 1.4 m/s, then stopped; running at 3.5 m/s, then
/// stopped.
const WALK: std::ops::Range<u32> = 60..240;
const RUN: std::ops::Range<u32> = 330..480;

/// What the player does at tick `t`, along `east` and `north` at the spawn (`up` local).
fn script(t: u32, east: DVec3, north: DVec3) -> Act {
    let mut act = match t {
        t if WALK.contains(&t) => Act::walk(east * 1.4),
        t if RUN.contains(&t) => Act::walk(east * 3.5),
        540..720 => {
            let a = f64::from(t - 540) / 60.0;
            let (s, c) = creeer_math::det::sin_cos(a);
            Act::walk((east * c + north * s) * 1.4)
        }
        720..800 => Act::walk(-east * 1.4),
        _ => Act::default(),
    };
    act.jump = t == 660;
    act
}

/// A frame drawn: its tick, the bones, the body's up and its speed along the ground (m/s),
/// whether it was stepping and which feet stood on the ground.
struct Drawn {
    tick: u32,
    bones: Vec<Frame64>,
    up: DVec3,
    speed: f64,
    stepping: bool,
    down: [bool; 2],
}

/// Each frame for the script's `ticks`.
fn played(ticks: u32) -> Vec<Drawn> {
    let terrain = terrain();
    let mut play = Play::new(world(terrain.clone()), "probe", &["ada"], &terrain);
    let spawn = play.server.world.spawn();
    let (east, north) = creeer_terrain::geology::deposits::tangent_frame(spawn.normalize());
    let ground = PlanetGround { terrain: &terrain };
    let mut animator = Animator::default();
    let mut frames = Vec::new();
    for t in 0..ticks {
        play.tick(&[script(t, east, north)]);
        let Some(pose) = play.client(0).avatar() else {
            continue;
        };
        let body = Body::Standing(pose.position, pose.orientation);
        let bones = animator.bones(1, &body, 1.0 / 60.0, &ground);
        let state = &animator.states[&1];
        frames.push(Drawn {
            tick: t,
            bones,
            up: pose.orientation * DVec3::Y,
            speed: state
                .velocity
                .reject_from(pose.orientation * DVec3::Y)
                .length(),
            stepping: state.stepping,
            down: state.feet.map_or([false; 2], |f| [f[0].down, f[1].down]),
        });
    }
    frames
}

/// Each bone's turn between frames relative to the pelvis (rad/s; the body's own turning is
/// not the limbs'), by frame pair.
fn rates(frames: &[Drawn]) -> Vec<Vec<f64>> {
    frames
        .windows(2)
        .map(|pair| {
            let (a, b) = (&pair[0].bones, &pair[1].bones);
            a.iter()
                .zip(b)
                .map(|(x, y)| {
                    let rx = a[0].rotation.inverse() * x.rotation;
                    let ry = b[0].rotation.inverse() * y.rotation;
                    rx.angle_between(ry) * 60.0
                })
                .collect()
        })
        .collect()
}

/// Each bone's spikes: how much faster it turned in a frame than in either frame beside it
/// (rad/s), by tick and bone. A motion speeding up and slowing down has none to speak of; a
/// pose that jumps for a frame has one as large as the jump.
fn spikes(frames: &[Drawn]) -> Vec<(u32, usize, f64)> {
    let mut out = Vec::new();
    for (k, w) in rates(frames).windows(3).enumerate() {
        for (bone, now) in w[1].iter().enumerate() {
            let over = now - w[0][bone].max(w[2][bone]);
            out.push((frames[k + 2].tick, bone, over));
        }
    }
    out
}

/// The feet set down once the body has come to rest after the input stops at `stop` (slower
/// than a person stands at, 0.15 m/s), until it stands still: steps taken in place.
fn steps_stopping(frames: &[Drawn], stop: u32) -> usize {
    let rest = frames
        .iter()
        .find(|f| f.tick >= stop && f.speed < 0.15)
        .map_or(u32::MAX, |f| f.tick);
    let mut steps = 0;
    for pair in frames.windows(2).filter(|p| p[1].tick >= rest) {
        if !pair[1].stepping {
            return steps;
        }
        steps += (0..2)
            .filter(|&s| pair[1].down[s] && !pair[0].down[s])
            .count();
    }
    usize::MAX
}

/// The trunk's lean forward from the body's up (degrees), the mean over `ticks`.
fn lean(frames: &[Drawn], ticks: std::ops::Range<u32>) -> f64 {
    let picked: Vec<_> = frames.iter().filter(|f| ticks.contains(&f.tick)).collect();
    let sum: f64 = picked
        .iter()
        .map(|f| {
            let trunk = f.bones[1].rotation * DVec3::Y;
            let forward = (f.bones[0].rotation * DVec3::Z)
                .reject_from(f.up)
                .normalize();
            creeer_math::det::asin(trunk.dot(forward)).to_degrees()
        })
        .sum();
    #[allow(clippy::cast_precision_loss)]
    let n = picked.len() as f64;
    sum / n
}

/// Each knee's bend over `ticks` (degrees, the thigh to the shank): its least and most with its
/// foot on the ground, and its most in all.
fn knees(frames: &[Drawn], ticks: std::ops::Range<u32>) -> [(f64, f64, f64); 2] {
    std::array::from_fn(|side| {
        let (thigh, shank) = ([6, 12][side], [7, 13][side]);
        let (mut least, mut most, mut all) = (f64::INFINITY, 0.0f64, 0.0f64);
        for f in frames.iter().filter(|f| ticks.contains(&f.tick)) {
            let bend = f.bones[thigh]
                .rotation
                .angle_between(f.bones[shank].rotation)
                .to_degrees();
            if f.down[side] {
                (least, most) = (least.min(bend), most.max(bend));
            }
            all = all.max(bend);
        }
        (least, most, all)
    })
}

#[test]
fn the_motion_has_no_glitches_stops_within_a_step_and_leans_running() {
    let frames = played(860);
    // No bone's pose jumps for a frame: in G1.3 setting off, jumping and landing each had one,
    // up to 48.5 rad/s over the frames beside it; now the most is 7.4 (a first fast swing as
    // the gait changes), gated a tenth above.
    let worst = spikes(&frames)
        .into_iter()
        .max_by(|a, b| a.2.total_cmp(&b.2))
        .expect("frames");
    assert!(worst.2 <= 8.1, "spike {worst:?}");
    // Stopping from a walk or a run: once the body has come to rest, at most the step that
    // brings the feet together, and none in place after it (Ridge et al. 2013: people stop
    // within about a step).
    for stop in [WALK.end, RUN.end] {
        let steps = steps_stopping(&frames, stop);
        assert!(steps <= 1, "{steps} feet set down stopping at tick {stop}");
    }
    // Running steadily the trunk leans forward as runners' do: 7.3° at 3.4 m/s (Teng and
    // Powers 2014); walking, upright. (G1.3 leant 9.2° running and 2.3° walking.)
    let running = lean(&frames, RUN.end - 40..RUN.end);
    assert!((running - 7.3).abs() < 0.5, "running, leaning {running}°");
    let walking = lean(&frames, WALK.end - 60..WALK.end);
    assert!(walking.abs() < 0.5, "walking, leaning {walking}°");
    // Walking, the stance knee straightens to about 5° (Perry), not crouched: within 5° of it.
    for (side, (least, _, _)) in knees(&frames, WALK.end - 60..WALK.end).iter().enumerate() {
        assert!(
            *least < 10.0,
            "side {side}: the stance knee bent {least}° at least"
        );
    }
}

#[test]
#[ignore = "measures the motion's glitches over the real server and client"]
fn measure_the_motion() {
    let frames = played(860);
    for (k, r) in rates(&frames).iter().enumerate() {
        for (bone, rate) in r.iter().enumerate().filter(|(_, r)| **r > 12.0) {
            eprintln!(
                "tick {}: bone {bone} turned at {rate:.1} rad/s",
                frames[k + 1].tick
            );
        }
    }
    let mut s = spikes(&frames);
    s.sort_by(|a, b| b.2.total_cmp(&a.2));
    for (t, bone, over) in s.iter().take(12) {
        eprintln!("spike at tick {t}: bone {bone}, {over:.1} rad/s over its neighbours");
    }
    for stop in [WALK.end, RUN.end] {
        eprintln!(
            "stopping at tick {stop}: {} feet set down",
            steps_stopping(&frames, stop)
        );
    }
    for (name, ticks) in [
        ("standing", 30..60),
        ("walking", WALK.end - 60..WALK.end),
        ("running", RUN.end - 40..RUN.end),
    ] {
        for (side, (least, most, all)) in knees(&frames, ticks).iter().enumerate() {
            eprintln!(
                "knee {name}, side {side}: {least:.0}–{most:.0}° on the ground, {all:.0}° at most"
            );
        }
    }
    eprintln!(
        "lean: running {:.1}°, walking {:.1}°",
        lean(&frames, RUN.end - 40..RUN.end),
        lean(&frames, WALK.end - 60..WALK.end)
    );
}
