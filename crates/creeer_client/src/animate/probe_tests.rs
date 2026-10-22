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
/// Then walking in a circle, jumping half-way round, walking back; a jump from standing still;
/// the script's length.
const CIRCLE: std::ops::Range<u32> = 660..840;
const JUMP: u32 = 780;
const BACK: std::ops::Range<u32> = 840..920;
const STANDING_JUMP: u32 = 980;
const TICKS: u32 = 1080;
/// A screen drawing faster than the server ticks.
const FPS: f64 = 144.0;

/// What the player does at tick `t`, along `east` and `north` at the spawn (`up` local).
fn script(t: u32, east: DVec3, north: DVec3) -> Act {
    let mut act = match t {
        t if WALK.contains(&t) => Act::walk(east * 1.4),
        t if RUN.contains(&t) => Act::walk(east * 3.5),
        t if CIRCLE.contains(&t) => {
            let a = f64::from(t - CIRCLE.start) / 60.0;
            let (s, c) = creeer_math::det::sin_cos(a);
            Act::walk((east * c + north * s) * 1.4)
        }
        t if BACK.contains(&t) => Act::walk(-east * 1.4),
        _ => Act::default(),
    };
    act.jump = t == JUMP || t == STANDING_JUMP;
    act
}

/// A frame drawn: its tick and the time since the last frame (s), the bones, the body's up and
/// its speed along the ground (m/s), whether it was stepping, which feet stood on the ground
/// and what it was doing.
struct Drawn {
    tick: u32,
    dt: f64,
    bones: Vec<Frame64>,
    up: DVec3,
    speed: f64,
    stepping: bool,
    down: [bool; 2],
    mode: super::pose::Mode,
}

/// Each frame for the script's `ticks`, drawn `fps` times a second: the server ticks at 60,
/// and the avatar, predicted a tick at a time, moves only when it does (a screen drawing
/// faster sees it still between ticks).
fn played(ticks: u32, fps: f64) -> Vec<Drawn> {
    let terrain = terrain();
    let mut play = Play::new(world(terrain.clone()), "probe", &["ada"], &terrain);
    let spawn = play.server.world.spawn();
    let (east, north) = creeer_terrain::geology::deposits::tangent_frame(spawn.normalize());
    let ground = PlanetGround { terrain: &terrain };
    let mut animator = Animator::default();
    let mut frames = Vec::new();
    let (dt, mut t, mut time, mut drawn_to) = (1.0 / fps, 0, 0.0, 0.0);
    while t < ticks {
        time += dt;
        while f64::from(t) / 60.0 < time && t < ticks {
            play.tick(&[script(t, east, north)]);
            drawn_to = f64::from(t) / 60.0;
            t += 1;
        }
        // The client drawing on between ticks, as each frame.
        play.draw_later(0, time - drawn_to);
        drawn_to = time;
        let Some(pose) = play.client(0).avatar() else {
            continue;
        };
        let body = Body::Standing(pose.position, pose.orientation);
        let bones = animator.bones(1, &body, dt, &ground);
        let state = &animator.states[&1];
        frames.push(Drawn {
            tick: t - 1,
            dt,
            bones,
            up: pose.orientation * DVec3::Y,
            speed: state
                .velocity
                .reject_from(pose.orientation * DVec3::Y)
                .length(),
            stepping: state.stepping,
            down: state.feet.map_or([false; 2], |f| [f[0].down, f[1].down]),
            mode: state.mode,
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
                    rx.angle_between(ry) / pair[1].dt
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

/// From the input stopping at `stop`: the feet set down until the body stands still, those once
/// it has come to rest (slower than a person stands at, 0.15 m/s: steps taken in place), and the
/// ticks it took; none if it never stood.
fn stopping(frames: &[Drawn], stop: u32) -> Option<(usize, usize, u32)> {
    let (mut steps, mut in_place) = (0, 0);
    for pair in frames.windows(2).filter(|p| p[1].tick > stop) {
        if !pair[1].stepping {
            return Some((steps, in_place, pair[1].tick - stop));
        }
        let set = (0..2)
            .filter(|&s| pair[1].down[s] && !pair[0].down[s])
            .count();
        steps += set;
        if pair[1].speed < 0.15 {
            in_place += set;
        }
    }
    None
}

/// The trunk's lean forward from the body's up (degrees), each frame over `ticks`.
fn leans(frames: &[Drawn], ticks: std::ops::Range<u32>) -> Vec<f64> {
    frames
        .iter()
        .filter(|f| ticks.contains(&f.tick))
        .map(|f| {
            let trunk = f.bones[1].rotation * DVec3::Y;
            let forward = (f.bones[0].rotation * DVec3::Z)
                .reject_from(f.up)
                .normalize();
            creeer_math::det::asin(trunk.dot(forward)).to_degrees()
        })
        .collect()
}

/// The trunk's lean, the mean over `ticks`.
fn lean(frames: &[Drawn], ticks: std::ops::Range<u32>) -> f64 {
    let all = leans(frames, ticks);
    #[allow(clippy::cast_precision_loss)]
    let n = all.len() as f64;
    all.iter().sum::<f64>() / n
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

/// The shortest any way of moving lasted before the next began (s), and when.
fn shortest_mode(frames: &[Drawn]) -> (f64, u32) {
    let (mut since, mut shortest) = (0.0, (f64::INFINITY, 0));
    for pair in frames.windows(2) {
        since += pair[1].dt;
        if pair[1].mode != pair[0].mode {
            if since < shortest.0 {
                shortest = (since, pair[1].tick);
            }
            since = 0.0;
        }
    }
    shortest
}

/// Leg `side`'s knee bend from straight (degrees), the model standing a little past it.
fn from_straight(side: usize, bend: f64) -> f64 {
    bend - super::skeleton::straight_knee(side).to_degrees()
}

#[test]
fn the_motion_has_no_glitches_stops_within_a_step_and_leans_running() {
    // On a screen drawing as the server ticks, and faster (the player's avatar then drawn
    // between its ticks): G1.4a found its glitches only at the faster.
    for fps in [60.0, FPS] {
        let frames = played(TICKS, fps);
        // No bone's pose jumps for a frame: in G1.3 setting off, jumping and landing each had
        // one, up to 48.5 rad/s over the frames beside it (drawn at 60 a second; at 144, the
        // player's own avatar moving a tick at a time, 90). Now at most 8.4 and 8.9, gated a
        // tenth above.
        let worst = spikes(&frames)
            .into_iter()
            .max_by(|a, b| a.2.total_cmp(&b.2))
            .expect("frames");
        assert!(worst.2 <= 9.8, "{fps} a second: spike {worst:?}");
        // Nothing flickers between ways of moving: each lasts a while.
        let (shortest, at) = shortest_mode(&frames);
        assert!(shortest > 0.1, "{fps} a second: tick {at}, {shortest} s");
        // Stopping from a walk or a run: standing within the measured time (from letting go:
        // walking 1.2 s, running 2.0 s, braking from 3.5 m/s about 0.9 of it), at most one foot
        // set down once the body is at rest (Ridge et al. 2013: an unplanned stop takes a
        // little more than a step), the trunk upright (Harper et al. 2022: braking, people set
        // the foot ahead and lower the body, the trunk upright or forward).
        for (stop, most) in [(WALK.end, 82), (RUN.end, 135)] {
            let (_, in_place, ticks) = stopping(&frames, stop).expect("it stands");
            assert!(
                in_place <= 1,
                "{fps}: {in_place} feet set down in place at {stop}"
            );
            assert!(ticks <= most, "{fps}: {ticks} ticks to stand at {stop}");
            let back = leans(&frames, stop..stop + 60)
                .into_iter()
                .fold(0.0, f64::min);
            assert!(
                back > -0.5,
                "{fps}: leaning back {back}° stopping at {stop}"
            );
        }
        // Running steadily the trunk leans forward as runners' do: 7.3° at 3.4 m/s (Teng and
        // Powers 2014); walking, upright. (G1.3 leant 9.2° running and 2.3° walking.)
        let running = lean(&frames, RUN.end - 40..RUN.end);
        assert!(
            (running - 7.3).abs() < 0.5,
            "{fps}: running, leaning {running}°"
        );
        let walking = lean(&frames, WALK.end - 60..WALK.end);
        assert!(walking.abs() < 0.5, "{fps}: walking, leaning {walking}°");
        // Walking, the stance knee straightens to about 5° (Perry), not crouched, never locked:
        // measured 5.9° and 10.0° (the ground under the walk not quite level), gated a tenth above.
        for (side, (least, _, _)) in knees(&frames, WALK.end - 60..WALK.end).iter().enumerate() {
            let bent = from_straight(side, *least);
            assert!(
                (4.5..11.0).contains(&bent),
                "{fps}: side {side}: knee {bent}° at least"
            );
        }
        // Down from a jump from standing still, it stands: no step taken, the legs straight.
        let last = frames.last().expect("frames");
        assert!(!last.stepping && last.speed < 0.01, "{fps}: still stepping");
        for (side, (_, most, _)) in knees(&frames, TICKS - 40..TICKS).iter().enumerate() {
            assert!(
                *most < 15.0,
                "{fps}: side {side}: knee bent {most}° standing"
            );
        }
    }
}

#[test]
#[ignore = "measures the motion's glitches over the real server and client"]
fn measure_the_motion() {
    let frames = played(TICKS, FPS);
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
        let back = leans(&frames, stop..stop + 60)
            .into_iter()
            .fold(0.0, f64::min);
        eprintln!(
            "stopping at tick {stop}: (feet set down, of them in place, ticks to stand) {:?}, \
             leaning back {back:.1}° at most",
            stopping(&frames, stop)
        );
    }
    for (name, ticks) in [
        ("standing", 30..60),
        ("walking", WALK.end - 60..WALK.end),
        ("running", RUN.end - 40..RUN.end),
        ("after the standing jump", TICKS - 40..TICKS),
    ] {
        for (side, (least, most, all)) in knees(&frames, ticks).iter().enumerate() {
            eprintln!(
                "knee {name}, side {side}: {:.1}–{:.1}° from straight on the ground, {:.0}° at most",
                from_straight(side, *least),
                from_straight(side, *most),
                from_straight(side, *all)
            );
        }
    }
    for p in frames.windows(2).filter(|p| p[0].mode != p[1].mode) {
        eprintln!("tick {}: {:?} to {:?}", p[1].tick, p[0].mode, p[1].mode);
    }
    eprintln!(
        "shortest way of moving: {:?} (s, tick)",
        shortest_mode(&frames)
    );
    if let Some(last) = frames.last() {
        eprintln!(
            "at the end: stepping {}, speed {:.3} m/s",
            last.stepping, last.speed
        );
    }
    eprintln!(
        "lean: running {:.1}°, walking {:.1}°",
        lean(&frames, RUN.end - 40..RUN.end),
        lean(&frames, WALK.end - 60..WALK.end)
    );
}
