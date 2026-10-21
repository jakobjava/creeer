//! Headless screenshots of the solar-system sandbox:
//! `creeer_client --system <home|orbit|moon|map|system|lagrange|transfer> <file.png>`.
//!
//! The simulation runs in real time on its own thread, as in the window; each view sets up
//! a scene with commands, waits for it, streams the terrain and renders one frame.

use crate::DemoMeshes;
use crate::app::system::load_universe;
use crate::system_cmd::{Command, Direction, Engine};
use crate::system_frame::{self, Built};
use crate::system_sim::SystemSnapshot;
use crate::system_view::SystemView;
use creeer_math::det;
use creeer_math::glam::DVec3;
use creeer_render::{Camera, DebugLines, Frame, Lighting, Renderer, Scene, TerrainFrame};
use creeer_sim::universe::terrains::BodyRef;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn wait(seconds: f64) {
    std::thread::sleep(Duration::from_secs_f64(seconds));
}

fn now(view: &SystemView) -> Arc<SystemSnapshot> {
    view.snapshot().current
}

/// Up direction (bubble axes) at a point of `body` where the star stands `elevation` degrees
/// above the horizon, and a horizontal direction across the light.
fn sunlit(s: &SystemSnapshot, body: usize, elevation: f64) -> (DVec3, DVec3) {
    let star = s.pose.point_from_inertial(s.bodies[0].state.0);
    let centre = s.pose.point_from_inertial(s.bodies[body].state.0);
    let day = (star - centre).normalize();
    let side = day.cross(DVec3::Y).normalize_or(DVec3::X);
    let tilt = (90.0 - elevation).to_radians();
    let up = (day * det::cos(tilt) + side.cross(day) * det::sin(tilt)).normalize();
    (up, side)
}

/// A camera `height` above the ground of the body the bubble turns with, at `up`, looking
/// along `side`.
fn ground_camera(s: &SystemSnapshot, up: DVec3, side: DVec3, height: f64) -> Option<Camera> {
    let anchor = s.anchor?;
    let terrain = s.terrains.lock().ok()?.loaded(anchor).cloned()?;
    let ground = up * (terrain.planet().datum.radius(up) + terrain.surface_height(up));
    let forward = (side - up * 0.12).normalize();
    Some(crate::planet_view::place(ground + up * height, forward))
}

/// Builds frames until the terrain has streamed; returns what the last one needs.
fn settle(
    view: &mut SystemView,
    renderer: &mut Renderer,
    (scene, lines, meshes): (&mut Scene, &mut DebugLines, &DemoMeshes),
    camera: &mut Camera,
    minimum: Duration,
) -> Built {
    let start = Instant::now();
    loop {
        let published = view.snapshot();
        view.update_camera(camera, &published.current);
        let built = system_frame::build(
            view,
            renderer,
            (scene, lines, meshes),
            camera,
            &published,
            1.0,
        );
        if (view.outstanding() == 0 && start.elapsed() > minimum)
            || start.elapsed() > Duration::from_secs(120)
        {
            return built;
        }
        wait(0.01);
    }
}

/// Renders one view to the file named after it.
pub fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let i = args
        .iter()
        .position(|a| a == "--system")
        .ok_or("--system")?;
    let name = args.get(i + 1).ok_or("--system needs a view")?.clone();
    let path = args
        .get(i + 2)
        .ok_or("--system needs an output file")?
        .clone();
    let mut view = SystemView::new(load_universe()?, crate::TICK_RATE_HZ);
    let mut renderer = pollster::block_on(Renderer::new_headless((1600, 900)))?;
    let meshes = DemoMeshes::register_for(&mut renderer);
    let (mut scene, mut lines) = (Scene::default(), DebugLines::default());
    wait(0.3);
    let s = now(&view);
    let home = system_frame::home(&s);
    let moon = (0..s.bodies.len())
        .find(|&k| {
            s.bodies[k].parent == Some(home) && matches!(s.bodies[k].which, BodyRef::Major(_))
        })
        .ok_or("home has no moon")?;
    let moon_distance = (s.bodies[moon].state.0 - s.bodies[home].state.0).length();
    let mut camera = Camera::new(DVec3::ZERO);
    match name.as_str() {
        "home" => {
            let (up, side) = sunlit(&s, home, 25.0);
            camera = ground_camera(&s, up, side, 40.0).ok_or("not on home")?;
            view.adopt_frame(&s);
            view.send(Command::FocusPoint(camera.position));
            let ahead = camera.forward();
            view.send(Command::Crate(camera.position + ahead * 15.0 + up * 3.0));
            view.send(Command::Ball(
                camera.position + ahead * 18.0 + side * 3.0 + up * 4.0,
            ));
            wait(2.0);
        }
        "orbit" | "map" | "transfer" | "lagrange" => {
            let (up, _) = sunlit(&s, home, 60.0);
            view.send(Command::FocusPoint(up * 6e5));
            view.send(Command::ShipInOrbit(150_000.0));
            view.follow = Some(DVec3::ZERO);
            wait(1.0);
            if name == "transfer" || name == "lagrange" {
                // Burn prograde until the orbit reaches out to the moon.
                view.send(Command::Engine(Engine {
                    thrust: 2e6,
                    direction: Direction::Prograde,
                    on: true,
                }));
                let start = Instant::now();
                while now(&view).focus.as_ref().is_none_or(|f| {
                    f.elements.e < 1.0 && f.elements.apoapsis() < moon_distance * 1.05
                }) && start.elapsed() < Duration::from_secs(60)
                {
                    wait(0.02);
                }
                view.send(Command::Engine(Engine::default()));
            }
            wait(1.0);
            if name == "orbit" {
                let s = now(&view);
                let f = s.focus.as_ref().ok_or("no ship")?;
                let (ship, _) = s.pose.from_inertial(f.state.0, f.state.1);
                let centre = s.pose.point_from_inertial(s.bodies[home].state.0);
                let up = (ship - centre).normalize();
                let along = (s.pose.rotation.conjugate() * (f.state.1 - s.bodies[home].state.1))
                    .normalize();
                // 150 km up the horizon lies 40° below level: look 55° down, past the ship.
                camera = crate::planet_view::place_up(
                    ship - along * 30.0 + up * 8.0,
                    (along - up * 1.45).normalize(),
                    up,
                );
                view.follow = None;
                view.adopt_frame(&s);
                view.send(Command::FocusPoint(camera.position));
            } else {
                view.map = true;
                view.map_camera.pitch = 1.2;
                view.map_camera.distance = if name == "map" {
                    4e6
                } else {
                    6.0 * moon_distance
                };
                if name == "lagrange" {
                    view.corotating = true;
                    view.selected = Some(BodyRef::Major(moon));
                    view.map_camera.pitch = 1.5;
                    view.map_camera.distance = 3.0 * moon_distance;
                }
                // Ask for the prediction in this frame and give the simulation time for it.
                for _ in 0..4 {
                    settle(
                        &mut view,
                        &mut renderer,
                        (&mut scene, &mut lines, &meshes),
                        &mut camera,
                        Duration::ZERO,
                    );
                    wait(0.6);
                }
            }
        }
        "moon" => {
            // Send the focus to the moon: the bubble follows, through free fall, onto it.
            let (up, _) = sunlit(&s, moon, 30.0);
            let r = s.bodies[moon].body.radius;
            let target = s.pose.point_from_inertial(s.bodies[moon].state.0) + up * (r + 3000.0);
            view.send(Command::FocusPoint(target));
            camera.position = target;
            let start = Instant::now();
            while now(&view).anchor != Some(BodyRef::Major(moon))
                && start.elapsed() < Duration::from_secs(10)
            {
                settle(
                    &mut view,
                    &mut renderer,
                    (&mut scene, &mut lines, &meshes),
                    &mut camera,
                    Duration::ZERO,
                );
                wait(0.05);
            }
            let s = now(&view);
            if s.anchor != Some(BodyRef::Major(moon)) {
                return Err(format!("the bubble did not reach the moon: {:?}", s.anchor).into());
            }
            let (up, side) = sunlit(&s, moon, 30.0);
            camera = ground_camera(&s, up, side, 80.0).ok_or("no moon terrain")?;
            view.adopt_frame(&s);
            view.send(Command::FocusPoint(camera.position));
            wait(0.3);
        }
        "system" => {
            view.map = true;
            view.selected = Some(BodyRef::Major(0));
            view.map_camera.pitch = 1.1;
            view.map_camera.distance = 3.2e11;
        }
        other => return Err(format!("unknown system view `{other}`").into()),
    }
    let built = settle(
        &mut view,
        &mut renderer,
        (&mut scene, &mut lines, &meshes),
        &mut camera,
        Duration::from_millis(700),
    );
    let mut lighting = match &built.sky {
        Some(sky) => sky.lighting(built.camera.position, built.sun, &Lighting::default()),
        None => system_frame::space_lighting(),
    };
    lighting.sun_direction = built.sun;
    let (draw, frames): (&[(u64, u8)], &[TerrainFrame]) = if view.map {
        (&[], &[])
    } else {
        (&view.draw, &view.terrain_frames)
    };
    let image = renderer.capture(Frame {
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
    image.save_png(std::path::Path::new(&path))?;
    if std::env::var_os("CREEER_DEBUG_SHOT").is_some() {
        eprintln!(
            "camera {:?} fwd {:?} near {:?} frames {:?} draws {}",
            built.camera.position,
            built.camera.forward(),
            view.near_body(),
            view.terrain_frames,
            view.draw.len()
        );
    }
    let s = now(&view);
    let stats = renderer.stats();
    let ship = s.focus.as_ref().map_or(String::new(), |f| {
        let radius = s.bodies[f.primary].body.radius;
        format!(
            "; ship {:.0} km up, periapsis {:.0} km, apoapsis {:.0} km, e {:.3}",
            f.altitude / 1e3,
            (f.elements.periapsis() - radius) / 1e3,
            (f.elements.apoapsis() - radius) / 1e3,
            f.elements.e
        )
    });
    println!(
        "saved {path} (`{name}`): t = {:.0} s, frame {:?}, {} terrain chunks, {} labels, prediction {} points{ship}",
        s.time,
        s.anchor,
        stats.terrain_chunks_drawn,
        built.labels.len(),
        s.prediction.1.points.len()
    );
    Ok(())
}
