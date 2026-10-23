//! Headless screenshots of the planet sandbox, for reviewing terrain and lighting:
//!
//! `creeer_client --planet <orbit|high|low|coast|surface|avatar|tunnel|night> <file.png> [--sun <deg>]`
//!
//! The camera looks at a land spot with the sun `--sun` degrees above its horizon (40° by
//! default; `night` puts it 30° below). The terrain is streamed until nothing is left to build
//! before the image is taken, so it shows the settled level of detail.

use crate::DemoMeshes;
use crate::physics_view;
use crate::planet_view::PlanetView;
use creeer_math::det;
use creeer_math::glam::DVec3;
use creeer_net::message::Command;
use creeer_render::{DebugLines, Frame, Lighting, Renderer, Scene};
use creeer_terrain::edits::{Brush, Edit};
use creeer_terrain::elevation::fibonacci_sphere;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Runs the game for `seconds` of real time (the hosted server steps on its own; the client
/// takes in what it is told), first until the client has joined.
fn play(view: &mut PlanetView, seconds: f64) -> Result<(), String> {
    let start = Instant::now();
    let step = Duration::from_millis(10);
    while view.session.connection.status() != &creeer_server::client::Status::Joined {
        if start.elapsed() > Duration::from_secs(10) {
            return Err("the hosted server did not welcome the client".into());
        }
        view.session
            .update(step.as_secs_f64(), (DVec3::ZERO, false, DVec3::ZERO));
        std::thread::sleep(step);
    }
    let start = Instant::now();
    while start.elapsed().as_secs_f64() < seconds {
        view.session
            .update(step.as_secs_f64(), (DVec3::ZERO, false, DVec3::ZERO));
        std::thread::sleep(step);
    }
    Ok(())
}

/// Renders one planet view and saves it.
pub fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let at = args
        .iter()
        .position(|a| a == "--planet")
        .expect("called with --planet");
    let view_name = args.get(at + 1).ok_or("--planet needs a view")?.clone();
    let path = args
        .get(at + 2)
        .ok_or("--planet needs a file name")?
        .clone();
    let mut sun_elevation = match args.iter().position(|a| a == "--sun") {
        Some(i) => args
            .get(i + 1)
            .and_then(|s| s.parse::<f64>().ok())
            .ok_or("--sun expects degrees")?,
        None => 40.0,
    };
    if view_name == "night" {
        sun_elevation = -30.0;
    }

    let mut view = PlanetView::singleplayer(true)?;
    let terrain = Arc::clone(&view.terrain);
    play(&mut view, 0.0)?;
    // Where players appear (land with relief at a temperate latitude, G1.3i): bodies are
    // simulated only near a player, and the noon sun stands 45-60° high there.
    let up = creeer_server::world::spawn_point(&terrain).normalize();
    view.day_fraction = day_for_sun(&view, up, sun_elevation);
    view.day_speed = 0.0;
    let (east, _) = creeer_terrain::geology::deposits::tangent_frame(up);
    let camera = match view_name.as_str() {
        "orbit" => view.orbit_camera(),
        // From orbit above the spot, where the sun is below the horizon: the night side, with
        // the terminator at the edge.
        "night" => crate::planet_view::place(up * terrain.planet().radius * 3.2, -up),
        "high" => view.ground_camera(up, 20_000.0, east, 25f64.to_radians()),
        "low" => view.ground_camera(up, 300.0, east, 12f64.to_radians()),
        // A coast (F6.6): low land with the sea 3 km on, looked at from 200 m up towards it.
        "coast" => {
            let (at, seaward) = fibonacci_sphere(20_000)
                .filter(|&d| (5.0..60.0).contains(&terrain.surface_height(d)))
                .find_map(|d| {
                    let (e, _) = creeer_terrain::geology::deposits::tangent_frame(d);
                    let n = d.cross(e);
                    (0..8)
                        .map(|k| {
                            let (s, c) = creeer_math::det::sin_cos(
                                f64::from(k) * std::f64::consts::FRAC_PI_4,
                            );
                            e * c + n * s
                        })
                        .find(|&t| terrain.surface_height((d + t * 6e-3).normalize()) < -20.0)
                        .map(|t| (d, t))
                })
                .ok_or("no coast found")?;
            view.day_fraction = day_for_sun(&view, at, sun_elevation);
            view.ground_camera(at, 200.0, seaward, 6f64.to_radians())
        }
        // The player's own character (G1.3e), from behind as the game follows it.
        "avatar" => {
            play(&mut view, 2.0)?;
            let at = (view.session.connection.avatar())
                .ok_or("no avatar")?
                .position;
            let up = at.normalize();
            let (east, _) = creeer_terrain::geology::deposits::tangent_frame(up);
            view.day_fraction = day_for_sun(&view, up, sun_elevation);
            let mut camera = view.ground_camera(up, 2.0, east, 12f64.to_radians());
            view.follow(&mut camera);
            camera
        }
        "surface" => {
            // Drop a few crates and balls in front of the camera and let them land.
            let ahead = view.ground_camera(up, 2.0, east, 0.0);
            for i in 0..6 {
                let side = up.cross(east).normalize() * (f64::from(i) - 2.5) * 2.2;
                let p = ahead.position + east * 14.0 + side + up * (3.0 + f64::from(i));
                let target = p.normalize()
                    * (terrain.planet().datum.radius(p) + terrain.surface_height(p) + 3.0);
                view.send(if i % 2 == 0 {
                    Command::Crate(target)
                } else {
                    Command::Ball(target)
                });
            }
            play(&mut view, 4.0)?;
            view.ground_camera(up, 2.5, east, 6f64.to_radians())
        }
        "tunnel" => {
            // Bore a tunnel into the ground and look into it from its mouth.
            let ground = up * (terrain.planet().datum.radius(up) + terrain.surface_height(up));
            let a = ground - up * 3.0 - east * 2.0;
            let b = a + east * 30.0 - up * 6.0;
            // Through the server, as a player digs: back here once it has done them.
            view.session
                .edit(Edit::Dig(Brush::Capsule { a, b, radius: 3.0 }));
            view.session.edit(Edit::Dig(Brush::Sphere {
                center: a - east * 2.0,
                radius: 4.5,
            }));
            play(&mut view, 0.5)?;
            if terrain.edits().len() < 2 {
                return Err("the server did not make the tunnel".into());
            }
            crate::planet_view::place(a - east * 1.5 + up * 0.5, (b - a).normalize())
        }
        other => return Err(format!("unknown planet view `{other}`").into()),
    };

    let mut renderer = pollster::block_on(Renderer::new_headless((1600, 900)))?;
    let meshes = DemoMeshes::register_for(&mut renderer);
    // Stream until settled.
    let start = Instant::now();
    loop {
        view.update_terrain(&mut renderer, camera.position);
        if view.streaming().1 == 0 && start.elapsed() > Duration::from_millis(500) {
            view.update_terrain(&mut renderer, camera.position);
            break;
        }
        if start.elapsed() > Duration::from_secs(120) {
            return Err("terrain streaming did not settle".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let streaming = start.elapsed();

    let sky = view.sky();
    let sun = view.sun_direction();
    let mut lighting = sky.lighting(camera.position, sun, &Lighting::default());
    lighting.shadow_distance = 250.0;
    let mut scene = Scene::default();
    view.session.update(0.0, (DVec3::ZERO, false, DVec3::ZERO));
    let snapshot = view.snapshot();
    physics_view::draw(
        &mut scene,
        &meshes,
        DVec3::ZERO,
        (&snapshot, &snapshot, 1.0),
        crate::demo::Overlay::Off,
    );
    // The players' characters (G1.3e).
    let mut people = crate::people::People::register(&mut renderer)?;
    let ground = crate::people_ground::PlanetGround {
        terrain: &view.terrain,
    };
    people.draw(&snapshot.people, 0.0, &ground, &mut scene);
    let image = renderer.capture(Frame {
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
    image.save_png(std::path::Path::new(&path))?;
    let stats = renderer.stats();
    println!(
        "saved {path} (view `{view_name}`): {} terrain chunks drawn, {} triangles, \
         streamed in {:.2} s, altitude {:.0} m, {} bodies",
        stats.terrain_chunks_drawn,
        stats.terrain_triangles,
        streaming.as_secs_f64(),
        view.altitude(camera.position),
        snapshot.counts.0,
    );
    // `--frames n`: the same view drawn n times more, each from the snapshot on (G1.7: what a
    // forest costs a frame).
    let frames = (args.iter().position(|a| a == "--frames"))
        .and_then(|i| args.get(i + 1)?.parse::<usize>().ok());
    if let Some(n) = frames {
        let mut stats = creeer_core::TimingStats::new(n);
        for _ in 0..n {
            let started = Instant::now();
            let snapshot = view.snapshot();
            let mut scene = Scene::default();
            let pair = (&snapshot, &snapshot, 1.0);
            physics_view::draw(
                &mut scene,
                &meshes,
                DVec3::ZERO,
                pair,
                crate::demo::Overlay::Off,
            );
            people.draw(&snapshot.people, 0.0, &ground, &mut scene);
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
            stats.push(started.elapsed());
        }
        let p = |q| stats.percentile_ms(q).unwrap_or(0.0);
        println!(
            "{n} frames: p50 {:.2} ms, p99 {:.2} ms; {} rods, {} blobs",
            p(50.0),
            p(99.0),
            snapshot.rods.len(),
            snapshot.blobs.len()
        );
    }
    Ok(())
}

/// The time of day (fraction) at which the sun stands `elevation` degrees above the horizon
/// at `up`, in the morning.
fn day_for_sun(view: &PlanetView, up: DVec3, elevation: f64) -> f64 {
    let target = det::sin(elevation.to_radians());
    let mut best = (f64::MAX, 0.0);
    for i in 0..4000 {
        let f = f64::from(i) / 4000.0;
        let sun = view.sun_direction_at(f);
        // Morning: the sun rises in the east (its height still growing).
        let rising = view.sun_direction_at(f + 1e-3).dot(up) > sun.dot(up);
        let error = (sun.dot(up) - target).abs();
        if rising && error < best.0 {
            best = (error, f);
        }
    }
    // Far from the equator or in winter, the sun may never stand that high: say so.
    let reached = det::asin(view.sun_direction_at(best.1).dot(up)).to_degrees();
    if (reached - elevation).abs() > 1.0 {
        eprintln!("the sun reaches {reached:.1}° here at most, not {elevation}°");
    }
    best.1
}

/// Flies from orbit down to the ground in `seconds` (real time, frames paced at 60 Hz) while
/// streaming terrain and rendering at 1080p off-screen; prints frame-time percentiles and the
/// number of hitches (frames over 33 ms). `creeer_client --planet-bench [seconds]`.
pub fn descent_bench(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let seconds: f64 = args
        .iter()
        .position(|a| a == "--planet-bench")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(40.0);
    let mut view = PlanetView::singleplayer(true)?;
    let terrain = Arc::clone(&view.terrain);
    let up = fibonacci_sphere(5000)
        .find(|&d| (300.0..1500.0).contains(&terrain.surface_height(d)))
        .ok_or("no land spot found")?;
    view.day_fraction = day_for_sun(&view, up, 35.0);
    view.day_speed = 0.0;
    let (east, _) = creeer_terrain::geology::deposits::tangent_frame(up);
    let mut renderer = pollster::block_on(Renderer::new_headless((1920, 1080)))?;
    let sky = view.sky();
    let scene = Scene::default();
    let lines = DebugLines::default();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // seconds of frames
    let frames = (seconds.max(1.0) * 60.0).round() as usize;
    let (top, bottom) = (1_100_000.0f64, 2.0f64);
    let mut stats = creeer_core::TimingStats::new(frames);
    // Lighting is integrated over the sky: as in the app, only when the height changed much.
    let mut lighting = Lighting::default();
    let mut lit_at = f64::NAN;
    let (mut hitches, mut worst, mut worst_at) = (0, 0.0f64, 0.0f64);
    let mut first_ms = 0.0;
    let start = Instant::now();
    for frame in 0..frames {
        let frame_start = Instant::now();
        // Altitude falls exponentially (equal time per factor of height), heading east and
        // pitching from straight down to the horizon.
        #[allow(clippy::cast_precision_loss)]
        let t = frame as f64 / frames as f64;
        let altitude = top * det::pow(bottom / top, t);
        let pitch = (90.0 - 85.0 * t).to_radians();
        let camera = view.ground_camera(up, altitude, east, pitch);
        view.update_terrain(&mut renderer, camera.position);
        if !(altitude / lit_at - 1.0).abs().lt(&0.05) {
            lighting = sky.lighting(camera.position, view.sun_direction(), &Lighting::default());
            lit_at = altitude;
        }
        renderer.render_offscreen(Frame {
            camera: &camera,
            scene: &scene,
            terrain: &view.draw,
            terrain_frames: &[],
            lighting: &lighting,
            lines: &lines,
            sky: Some(&sky),
            heatmap: None,
            overlay: None,
            clear_color: crate::BACKGROUND,
        });
        let took = frame_start.elapsed();
        let ms = took.as_secs_f64() * 1e3;
        if frame == 0 {
            // Setup: pipelines warm up and the sky table is integrated (loading, not flight).
            first_ms = ms;
            continue;
        }
        stats.push(took);
        if ms > 33.0 {
            hitches += 1;
        }
        if ms > worst {
            (worst, worst_at) = (ms, altitude);
        }
        // Pace at 60 Hz so the terrain workers get real time, as in the game.
        if let Some(rest) = Duration::from_secs_f64(1.0 / 60.0).checked_sub(took) {
            std::thread::sleep(rest);
        }
    }
    let p = |q| stats.percentile_ms(q).unwrap_or(0.0);
    println!(
        "descent {top:.0} m -> {bottom:.0} m in {:.1} s ({frames} frames, 1080p, {}): \
         first frame (setup) {first_ms:.0} ms, then frame p50 {:.2} ms, p99 {:.2} ms, \
         max {worst:.2} ms at {worst_at:.0} m; \
         {hitches} frames over 33 ms; {} chunks drawn at the end, {} triangles",
        start.elapsed().as_secs_f64(),
        renderer.adapter_info().name,
        p(50.0),
        p(99.0),
        renderer.stats().terrain_chunks_drawn,
        renderer.stats().terrain_triangles,
    );
    Ok(())
}
