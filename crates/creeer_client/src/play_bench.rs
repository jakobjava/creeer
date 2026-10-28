//! `creeer_client --play-bench <knap|fell> [--seconds <s>]` (2026-10-28): a singleplayer game as
//! the window plays it, headless, its frames timed phase by phase. The server is hosted on its
//! own thread in real time (`host.rs`); each frame does what `app/planet.rs` does while walking
//! (the session's ticks and its prediction, the terrain, trees and loose things about the
//! camera, the snapshot drawn, the scene built, the crosshair's aim, the frame rendered at
//! 1600 × 900 off-screen), paced at 60 a second. A script plays the player:
//!
//! - `knap`: *toolmaking*'s world; to the first station, the lighter stone taken up, the core
//!   struck four times, as the user knapped (2026-10-28: "30 ms frames");
//! - `fell`: *lumberjack*'s world; the steel axe, the nearest notched tree cut until it goes
//!   over, then its branches struck off where it lies ("slow frames", "the tree flashes").
//!
//! It prints the frames over budget with their phases, each phase's p50 and worst, and the
//! frames the felled trees' stems are drawn shorter than the frame before and after (a flash).

use crate::DemoMeshes;
use crate::physics_view;
use crate::planet_view::{PlanetView, place};
use creeer_math::glam::DVec3;
use creeer_net::entity::Kind;

use creeer_render::{DebugLines, Frame, Lighting, Renderer, Scene};
use creeer_server::client::Status;

use std::time::{Duration, Instant};

/// The phases of a frame, as timed.
const PHASES: [&str; 6] = ["session", "terrain", "snapshot", "scene", "aim", "render"];
/// The frame budget at 60 Hz (ms).
const BUDGET: f64 = 16.7;

/// Runs the bench.
///
/// # Errors
/// If the planet or the renderer cannot be made, or the scene is unknown.
pub fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let at = (args.iter().position(|a| a == "--play-bench")).expect("called with --play-bench");
    let what = args.get(at + 1).map_or("knap", String::as_str).to_owned();
    let seconds: f64 = (args.iter().position(|a| a == "--seconds"))
        .and_then(|i| args.get(i + 1)?.parse().ok())
        .unwrap_or(if what == "fell" { 45.0 } else { 16.0 });
    let name = match what.as_str() {
        "knap" => "toolmaking",
        "fell" => "lumberjack",
        other => return Err(format!("no play bench `{other}` (knap, fell)").into()),
    };
    let data = crate::content::data();
    let terrain = creeer_server::world::load_terrain(&data, crate::planet_net::PLANET)?;
    let scene = creeer_server::play::scenes::scene(name).ok_or("no such scene")?;
    let mut world = (scene.build)(terrain);
    // The user's game is survival; the feller takes the creative steel axe.
    world.creative = what == "fell";
    let mut view = PlanetView::hosting(world)?;
    view.walking = true;
    let mut renderer = pollster::block_on(Renderer::new_headless((1600, 900)))?;
    let meshes = DemoMeshes::register_for(&mut renderer);
    let mut people = crate::people::People::register(&mut renderer)?;
    let sky = view.sky();
    let mut lighting = Lighting::default();
    let mut camera = view.orbit_camera();
    let mut player = crate::play_scripts::Player::default();
    let mut times: Vec<[f64; 6]> = Vec::new();
    let mut drawn: Vec<(f64, usize)> = Vec::new();
    let (mut ticks, mut ticked) = (Vec::new(), 0);
    let every = args.iter().any(|a| a == "--each-second");
    let frame = Duration::from_secs_f64(1.0 / 60.0);
    let (mut next, mut last, mut joined) = (Instant::now(), Instant::now(), None);
    let start = Instant::now();
    while start.elapsed().as_secs_f64() < seconds + 20.0 {
        if let Some(rest) = next.checked_duration_since(Instant::now()) {
            std::thread::sleep(rest);
        }
        next += frame;
        let dt = last.elapsed().as_secs_f64();
        last = Instant::now();
        let mut ms = [0.0; 6];
        let mut lap = Instant::now();
        let mut mark = |k: usize, lap: &mut Instant| {
            ms[k] = lap.elapsed().as_secs_f64() * 1e3;
            *lap = Instant::now();
        };
        let me = view.session.connection.avatar().map(|a| a.position);
        let ready = view.session.connection.status() == &Status::Joined && me.is_some();
        let since = ready.then(|| *joined.get_or_insert_with(Instant::now));
        let (walk, look) = match (since, me) {
            (Some(s), Some(me)) => {
                player.t = s.elapsed().as_secs_f64();
                if what == "fell" {
                    crate::play_scripts::fell(&mut view, &mut player, me)
                } else {
                    crate::play_scripts::knap(&mut view, &mut player, me)
                }
            }
            _ => (DVec3::ZERO, DVec3::ZERO),
        };
        view.session.update(dt, (walk, false, DVec3::ZERO));
        mark(0, &mut lap);
        if let Some(me) = view.session.connection.avatar().map(|a| a.position) {
            let up = me.normalize();
            let eye = me + up * 1.7;
            let to = (look - eye).normalize_or(up.any_orthonormal_vector());
            camera = place(eye - to * 2.5 + up * 0.5, to);
        }
        view.update_terrain(&mut renderer, camera.position);
        mark(1, &mut lap);
        let snapshot = view.snapshot();
        mark(2, &mut lap);
        let mut scene = Scene::default();
        physics_view::draw(
            &mut scene,
            &meshes,
            DVec3::ZERO,
            (&snapshot, &snapshot, 1.0),
            crate::demo::Overlay::Off,
        );
        let ground = crate::people_ground::PlanetGround {
            terrain: &view.terrain,
        };
        people.draw(&snapshot.people, dt, &ground, &mut scene);
        mark(3, &mut lap);
        let _ = crate::planet_aim::target(&view, &camera);
        mark(4, &mut lap);
        if times.is_empty() {
            lighting = sky.lighting(camera.position, view.sun_direction(), &Lighting::default());
        }
        renderer.render_offscreen(Frame {
            camera: &camera,
            scene: &scene,
            terrain: &view.draw,
            terrain_frames: &[],
            lighting: &lighting,
            lines: &DebugLines::default(),
            sky: Some(&sky),
            heatmap: None,
            overlay: None,
            clear_color: crate::BACKGROUND,
        });
        mark(5, &mut lap);
        // The hosted server's ticks since the last frame (on its own thread).
        let (stats, tick) =
            (view.session.tick_times()).unwrap_or_else(|| (creeer_core::TimingStats::new(1), 0));
        let new = usize::try_from(tick.saturating_sub(ticked)).unwrap_or(0);
        ticked = tick;
        let fresh: Vec<f64> = stats
            .samples_ms()
            .skip(stats.len().saturating_sub(new))
            .collect();
        let server = fresh.iter().copied().fold(0.0, f64::max);
        if let Some(s) = since {
            if s.elapsed().as_secs_f64() > seconds {
                break;
            }
            let t = s.elapsed().as_secs_f64();
            let total: f64 = ms.iter().sum();
            if total > BUDGET || server > BUDGET {
                let parts: Vec<String> = (PHASES.iter().zip(ms))
                    .map(|(n, m)| format!("{n} {m:.1}"))
                    .collect();
                let noted = view.session.connection.noted.as_ref().map(|n| n.0.clone());
                println!(
                    "{t:6.2} s: {total:5.1} ms ({}); server's worst tick since {server:.1} ms; {noted:?}",
                    parts.join(", ")
                );
            }
            times.push(ms);
            ticks.extend(fresh);
            drawn.push((t, felled_rods(&view)));
            // A line a second (`--each-second`): each phase's worst in it.
            if every && times.len().is_multiple_of(60) {
                let last = &times[times.len() - 60..];
                let worst: Vec<String> = (PHASES.iter().enumerate())
                    .map(|(k, n)| {
                        format!("{n} {:.1}", last.iter().map(|m| m[k]).fold(0.0, f64::max))
                    })
                    .collect();
                let server = ticks.iter().rev().take(60).copied().fold(0.0, f64::max);
                println!(
                    "{t:6.2} s, worst of the second: {}; server {server:.1}",
                    worst.join(", ")
                );
            }
        }
    }
    report(&times, &drawn);
    let pct = |q: f64| percentile(ticks.clone(), q);
    println!(
        "server    p50 {:6.2} p99 {:6.2} worst {:6.2} ms ({} ticks, on its own thread)",
        pct(50.0),
        pct(99.0),
        pct(100.0),
        ticks.len()
    );
    Ok(())
}

/// The value at percentile `q` (0–100) of `v`.
fn percentile(mut v: Vec<f64>, q: f64) -> f64 {
    v.sort_by(f64::total_cmp);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let i = ((q / 100.0) * (v.len().max(1) - 1) as f64).round() as usize;
    v.get(i).copied().unwrap_or(0.0)
}

/// The rods of the felled trees' parts drawn this frame (their stems and branches).
fn felled_rods(view: &PlanetView) -> usize {
    let c = &view.session.connection;
    let poses = c.poses();
    (c.kinds.iter())
        .filter(|(id, k)| matches!(k, Kind::Tree(_)) && poses.contains_key(id))
        .count()
}

fn report(times: &[[f64; 6]], drawn: &[(f64, usize)]) {
    let pct = percentile;
    for (k, name) in PHASES.iter().enumerate() {
        let v: Vec<f64> = times.iter().map(|t| t[k]).collect();
        println!(
            "{name:9} p50 {:6.2} p99 {:6.2} worst {:6.2} ms",
            pct(v.clone(), 50.0),
            pct(v.clone(), 99.0),
            pct(v, 100.0)
        );
    }
    let total: Vec<f64> = times.iter().map(|t| t.iter().sum()).collect();
    let over = total.iter().filter(|t| **t > BUDGET).count();
    println!(
        "frame     p50 {:6.2} p99 {:6.2} worst {:6.2} ms; {over} of {} frames over {BUDGET} ms",
        pct(total.clone(), 50.0),
        pct(total.clone(), 99.0),
        pct(total, 100.0),
        times.len()
    );
    // A felled part drawn by none for a frame or more (fewer drawn, then as many again within
    // a fifth of a second: one replaced by its pieces): when, and for how many frames.
    let mut flashes = Vec::new();
    for i in 1..drawn.len() {
        if drawn[i].1 >= drawn[i - 1].1 {
            continue;
        }
        let back = (i..drawn.len().min(i + 12)).find(|&j| drawn[j].1 >= drawn[i - 1].1);
        if let Some(j) = back {
            flashes.push((drawn[i].0, j - i));
        }
    }
    println!("felled parts drawn by none (s, frames): {flashes:.2?}");
}
