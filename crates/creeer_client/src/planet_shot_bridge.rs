//! Frames of the bridge site (G2.11, ADR-0039), for reviewing what the player sees:
//!
//! `creeer_client --planet bridge <file.png> [--built] [--blocks <n>] [--weights <n>]
//! [--after <s>] [--save <file>] [--look <near|side|deck|high>] [--overlay]`
//!
//! - without `--save`: the site as `--bridge` starts it, its parts settled; with `--built`, the
//!   *bridge_demo* scene's footbridge nailed across the gully; then `--blocks` granite blocks
//!   and `--weights` 1 t test weights set on its deck from the kit, as a player sets them, and
//!   `--after` seconds more (0.5 by default);
//! - `--save <file>`: a world the *bridge_demo* play scene saved at one of its moments
//!   (`BRIDGE_SAVES`), loaded into the site's game and played on `--after` seconds (1 by default)
//!   in real time, as a player would see it go on;
//! - `--look`: from the player's eyes on the near bank (`near`, the default), from 5 m along the
//!   gully at the bank's height (`side`), from the far end of the deck (`deck`), or from above
//!   behind the start (`high`);
//! - `--overlay`: the creative stress overlay on (O in the game), with its legend.
//!
//! What failed is marked and told as the game shows it on a creative server, overlay or not.

use crate::DemoMeshes;
use crate::bridge_site;
use crate::physics_view;
use crate::planet_shot_build::play;
use crate::planet_view::PlanetView;
use creeer_math::glam::DVec3;
use creeer_render::{Camera, DebugLines, Frame, Lighting, OverlayFrame, Renderer, Scene};
use creeer_server::world::site::{BridgeSite, Inspection};
use creeer_ui::egui;
use std::time::{Duration, Instant};

/// The frame's size (pixels).
const SIZE: (u32, u32) = (1600, 900);

/// Renders one frame of the bridge site and saves it.
///
/// # Errors
/// If the game cannot be made or the file written.
pub fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let value = |f: &str| args.windows(2).find(|w| w[0] == f).map(|w| w[1].clone());
    let at = args
        .iter()
        .position(|a| a == "--planet")
        .ok_or("--planet")?;
    let path = args
        .get(at + 2)
        .ok_or("--planet bridge needs a file name")?
        .clone();
    let built = args.iter().any(|a| a == "--built");
    let (mut view, site) = if built {
        bridge_site::view_built()?
    } else {
        bridge_site::view()?
    };
    view.day_speed = 0.0;
    play(&mut view, 1.0)?;
    // The gully's edits told before the terrain is streamed (else it is streamed undug).
    for _ in 0..100 {
        if view.terrain.edits().len() >= 2 {
            break;
        }
        play(&mut view, 0.1)?;
    }
    let look = value("--look").unwrap_or_else(|| "near".into());
    let camera = camera(&site, &look)?;
    let mut renderer = pollster::block_on(Renderer::new_headless(SIZE))?;
    let meshes = DemoMeshes::register_for(&mut renderer);
    // The terrain first: the world goes on in real time, and the frame is of a moment.
    settle_terrain(&mut view, &mut renderer, camera.position)?;
    view.stress.on = args.iter().any(|a| a == "--overlay");
    let _ = view.session.inspection(view.stress.on);
    if let Some(save) = value("--save") {
        view.session.load(std::path::Path::new(&save))?;
        let after = value("--after").and_then(|s| s.parse().ok()).unwrap_or(1.0);
        play(&mut view, after)?;
    } else {
        // The parts settle where they were laid.
        play(&mut view, 3.0)?;
        let count = |f: &str| value(f).and_then(|n| n.parse::<u32>().ok()).unwrap_or(0);
        load(&mut view, &site, count("--blocks"), count("--weights"))?;
        let after = value("--after").and_then(|s| s.parse().ok()).unwrap_or(0.5);
        play(&mut view, after)?;
    }
    view.update_terrain(&mut renderer, camera.position);
    let seen = view.session.inspection(view.stress.on).unwrap_or_default();
    let sky = view.sky();
    let mut lighting = sky.lighting(camera.position, view.sun_direction(), &Lighting::default());
    lighting.shadow_distance = 250.0;
    let mut scene = Scene::default();
    let mut snapshot = view.snapshot();
    view.stress.leave_out(&mut snapshot.beams);
    let pair = (&snapshot, &snapshot, 1.0);
    physics_view::draw(
        &mut scene,
        &meshes,
        DVec3::ZERO,
        pair,
        crate::demo::Overlay::Off,
    );
    view.stress
        .draw(&mut scene, &meshes, &view.session.connection, &seen);
    let mut people = crate::people::People::register(&mut renderer)?;
    // From the eyes, the player's own body is not drawn (as the game's first person).
    let own = (view.session.connection.welcome.as_ref()).map(|w| w.avatar.0);
    let drawn: Vec<_> = (snapshot.people.iter())
        .filter(|p| !(look == "near" && Some(p.key) == own))
        .cloned()
        .collect();
    let ground = crate::people_ground::PlanetGround {
        terrain: &view.terrain,
    };
    people.draw(&drawn, 0.0, &ground, &mut scene);
    let tick = view.session.tick_times().map_or(0, |t| t.1);
    let ui = egui::Context::default();
    let mut output = ui.run_ui(raw_input(), |ui| hud(ui.ctx(), &view, &camera, &seen, tick));
    let primitives = ui.tessellate(output.shapes, output.pixels_per_point);
    let image = renderer.capture(Frame {
        camera: &camera,
        scene: &scene,
        terrain: &view.draw,
        terrain_frames: &[],
        lighting: &lighting,
        lines: &DebugLines::default(),
        sky: Some(&sky),
        heatmap: view.stress.heatmap().as_ref(),
        overlay: Some(OverlayFrame {
            primitives: &primitives,
            textures_delta: &mut output.textures_delta,
            pixels_per_point: output.pixels_per_point,
        }),
        clear_color: crate::BACKGROUND,
    });
    image.save_png(std::path::Path::new(&path))?;
    println!(
        "saved {path} (bridge, {look}): {} failures told, {} structures shown",
        seen.failures.len(),
        seen.shares.len()
    );
    Ok(())
}

/// The deck loaded from the kit as the player loads it (I, aimed at the deck): `blocks` granite
/// blocks along its middle, one each 0.5 s, then `weights` 1 t test weights on its middle, one
/// each 3 s.
fn load(view: &mut PlanetView, site: &BridgeSite, blocks: u32, weights: u32) -> Result<(), String> {
    let data = crate::content::data();
    let up = site.middle.normalize();
    // The deck's top: the stringers (150 mm) and a board (50 mm) on the banks' ground.
    let deck = |x: f64, y: f64| site.middle + site.across * x + site.along * y + up * 0.2;
    let kit = |name: &str| BridgeSite::kit_index(&data, name).ok_or(format!("no {name}"));
    for n in 0..blocks {
        let (x, y) = (
            0.5 * f64::from(n % 4) - 0.75,
            0.3 * f64::from(n / 4 % 3) - 0.3,
        );
        let item = kit("granite block")?;
        view.send(creeer_net::message::Command::Kit {
            item,
            at: deck(x, y) + up * 0.09,
        });
        play(view, 0.5)?;
    }
    for _ in 0..weights {
        let item = kit("test weight")?;
        view.send(creeer_net::message::Command::Kit {
            item,
            at: deck(0.0, 0.0) + up * 0.31,
        });
        play(view, 3.0)?;
    }
    if let Some((why, _)) = &view.session.connection.refused {
        println!("the last load refused: {why}");
    }
    Ok(())
}

/// The camera of `look`.
fn camera(site: &BridgeSite, look: &str) -> Result<Camera, String> {
    let up = site.middle.normalize();
    let place = crate::planet_camera::place;
    let towards = |from: DVec3, to: DVec3| place(from, (to - from).normalize());
    let middle = site.middle;
    Ok(match look {
        "near" => bridge_site::camera(site),
        "side" => towards(middle + site.along * 5.0 + up * 0.6, middle - up * 0.4),
        "deck" => towards(
            middle + site.across * 1.6 + up * 1.7,
            middle - site.across * 1.0 - up * 0.2,
        ),
        "high" => towards(site.stand - site.across * 2.5 + up * 4.5, middle),
        other => return Err(format!("no look `{other}` at the bridge")),
    })
}

/// Streams the terrain about `at` until nothing is left to build.
fn settle_terrain(view: &mut PlanetView, renderer: &mut Renderer, at: DVec3) -> Result<(), String> {
    let start = Instant::now();
    loop {
        view.update_terrain(renderer, at);
        if view.streaming().1 == 0 && start.elapsed() > Duration::from_millis(500) {
            view.update_terrain(renderer, at);
            return Ok(());
        }
        if start.elapsed() > Duration::from_secs(120) {
            return Err("terrain streaming did not settle".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// The screen as egui is told it.
fn raw_input() -> egui::RawInput {
    #[allow(clippy::cast_precision_loss)] // pixels
    let size = egui::vec2(SIZE.0 as f32, SIZE.1 as f32);
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
        ..egui::RawInput::default()
    }
}

/// What the game draws over the frame here: the overlay's legend, and what failed.
fn hud(ctx: &egui::Context, view: &PlanetView, camera: &Camera, seen: &Inspection, tick: u64) {
    if let Some(scale) = view.stress.heatmap() {
        creeer_ui::heatmap_legend(ctx, &scale, "Share of what breaks it", "%");
    }
    crate::stress_view::hud(ctx, camera, seen, tick);
}
