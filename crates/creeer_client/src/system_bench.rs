//! `creeer_client --system-bench`: frame times of the solar-system sandbox at 1080p, paced at
//! 60 Hz as in the game, while the simulation runs on its own thread: following a ship in
//! low orbit (home's terrain streaming below), the map, and the map at 10⁵× warp.

use crate::DemoMeshes;
use crate::app::system::load_universe;
use crate::system_cmd::Command;
use crate::system_frame;
use crate::system_view::SystemView;
use creeer_math::glam::DVec3;
use creeer_render::{Camera, DebugLines, Frame, Lighting, Renderer, Scene, TerrainFrame};
use std::time::{Duration, Instant};

fn phase(
    name: &str,
    view: &mut SystemView,
    renderer: &mut Renderer,
    meshes: &DemoMeshes,
    camera: &mut Camera,
    frames: usize,
) {
    let (mut scene, mut lines) = (Scene::default(), DebugLines::default());
    let mut stats = creeer_core::TimingStats::new(frames);
    let (mut hitches, mut worst) = (0, 0.0f64);
    let mut lighting = Lighting::default();
    let mut first = None;
    for frame in 0..frames {
        let start = Instant::now();
        let published = view.snapshot();
        view.update_camera(camera, &published.current);
        if let (Some(offset), Some(focus)) = (view.follow, &published.current.focus) {
            let s = &published.current;
            let (ship, v) = s.pose.from_inertial(focus.state.0, focus.state.1);
            let _ = offset;
            let up =
                (ship - s.pose.point_from_inertial(s.bodies[focus.primary].state.0)).normalize();
            *camera = crate::planet_view::place_up(
                ship - v.normalize() * 30.0 + up * 8.0,
                (v.normalize() - up * 1.45).normalize(),
                up,
            );
        }
        let built = system_frame::build(
            view,
            renderer,
            (&mut scene, &mut lines, meshes),
            camera,
            &published,
            published.alpha(Instant::now()),
        );
        // Lighting is integrated over the sky: as in the app, only now and then.
        if frame % 30 == 0 {
            lighting = match &built.sky {
                Some(sky) => sky.lighting(built.camera.position, built.sun, &Lighting::default()),
                None => system_frame::space_lighting(),
            };
        }
        lighting.sun_direction = built.sun;
        let (draw, frames): (&[(u64, u8)], &[TerrainFrame]) = if view.map {
            (&[], &[])
        } else {
            (&view.draw, &view.terrain_frames)
        };
        renderer.render_offscreen(Frame {
            camera: &built.camera,
            scene: &scene,
            terrain: draw,
            terrain_frames: frames,
            lighting: &lighting,
            lines: &lines,
            sky: built.sky.as_ref(),
            heatmap: None,
            overlay: None,
            clear_color: crate::BACKGROUND,
        });
        let took = start.elapsed();
        let ms = took.as_secs_f64() * 1e3;
        if first.is_none() {
            // The first frame of a view sets things up (pipelines, tables): loading, not play.
            first = Some(ms);
            continue;
        }
        stats.push(took);
        hitches += usize::from(ms > 33.0);
        worst = worst.max(ms);
        if let Some(rest) = Duration::from_secs_f64(1.0 / 60.0).checked_sub(took) {
            std::thread::sleep(rest);
        }
    }
    let p = |q| stats.percentile_ms(q).unwrap_or(0.0);
    let ticks = view.sim.tick_times();
    println!(
        "{name:<28} first {:.0} ms, then frame p50 {:.2} ms, p99 {:.2} ms, max {worst:.1} ms, {hitches} over 33 ms; \
         sim tick p50 {:.2} ms, p99 {:.2} ms",
        first.unwrap_or(0.0),
        p(50.0),
        p(99.0),
        ticks.percentile_ms(50.0).unwrap_or(0.0),
        ticks.percentile_ms(99.0).unwrap_or(0.0),
    );
}

/// Runs the benchmark.
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut view = SystemView::new(load_universe()?, crate::TICK_RATE_HZ);
    let mut renderer = pollster::block_on(Renderer::new_headless((1920, 1080)))?;
    let meshes = DemoMeshes::register_for(&mut renderer);
    std::thread::sleep(Duration::from_millis(300));
    let s = view.snapshot().current;
    let home = system_frame::home(&s);
    let up = (s.pose.point_from_inertial(s.bodies[0].state.0)
        - s.pose.point_from_inertial(s.bodies[home].state.0))
    .normalize();
    view.send(Command::FocusPoint(up * 6e5));
    view.send(Command::ShipInOrbit(150_000.0));
    view.follow = Some(DVec3::ZERO);
    std::thread::sleep(Duration::from_millis(500));
    let mut camera = Camera::new(DVec3::ZERO);
    view.adopt_frame(&view.snapshot().current);
    println!(
        "solar-system sandbox, 1080p, {}",
        renderer.adapter_info().name
    );
    phase(
        "following a ship in orbit",
        &mut view,
        &mut renderer,
        &meshes,
        &mut camera,
        600,
    );
    view.map = true;
    view.map_camera.distance = 3e7;
    phase("map", &mut view, &mut renderer, &meshes, &mut camera, 300);
    view.send(Command::Warp(1e5));
    phase(
        "map at 10^5x warp",
        &mut view,
        &mut renderer,
        &meshes,
        &mut camera,
        300,
    );
    Ok(())
}
