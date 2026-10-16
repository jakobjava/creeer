//! Headless screenshots of the planet sandbox, for reviewing terrain and lighting:
//!
//! `creeer_client --planet <orbit|high|low|surface|tunnel|night> <file.png> [--sun <deg>]`
//!
//! The camera looks at a land spot with the sun `--sun` degrees above its horizon (40° by
//! default; `night` puts it 30° below). The terrain is streamed until nothing is left to build
//! before the image is taken, so it shows the settled level of detail.

use crate::DemoMeshes;
use crate::physics_view;
use crate::planet_sim::Command;
use crate::planet_view::{PlanetView, load_terrain};
use creeer_math::det;
use creeer_math::glam::DVec3;
use creeer_render::{DebugLines, Frame, Lighting, Renderer, Scene};
use creeer_terrain::edits::{Brush, Edit};
use creeer_terrain::elevation::fibonacci_sphere;
use std::time::{Duration, Instant};

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

    let terrain = load_terrain()?;
    let mut view = PlanetView::new(terrain.clone(), crate::TICK_RATE_HZ);
    // A land spot with relief: the first point between 300 and 1500 m above the datum.
    let up = fibonacci_sphere(5000)
        .find(|&d| (300.0..1500.0).contains(&terrain.surface_height(d)))
        .ok_or("no land spot found")?;
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
            std::thread::sleep(Duration::from_secs(4));
            view.ground_camera(up, 2.5, east, 6f64.to_radians())
        }
        "tunnel" => {
            // Bore a tunnel into the ground and look into it from its mouth.
            let ground = up * (terrain.planet().datum.radius(up) + terrain.surface_height(up));
            let a = ground - up * 3.0 - east * 2.0;
            let b = a + east * 30.0 - up * 6.0;
            terrain.edit(Edit::Dig(Brush::Capsule { a, b, radius: 3.0 }));
            terrain.edit(Edit::Dig(Brush::Sphere {
                center: a - east * 2.0,
                radius: 4.5,
            }));
            crate::planet_view::place(a - east * 1.5 + up * 0.5, (b - a).normalize())
        }
        other => return Err(format!("unknown planet view `{other}`").into()),
    };

    let mut renderer = pollster::block_on(Renderer::new_headless((1600, 900)))?;
    let meshes = DemoMeshes::register(&mut renderer.meshes);
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
    let snapshot = view.snapshot().current;
    physics_view::draw(
        &mut scene,
        &meshes,
        DVec3::ZERO,
        (&snapshot.bodies, &snapshot.bodies, 1.0),
        crate::demo::Overlay::Off,
    );
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
        snapshot.bodies.counts.0,
    );
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
    let terrain = load_terrain()?;
    let mut view = PlanetView::new(terrain.clone(), crate::TICK_RATE_HZ);
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
