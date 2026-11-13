//! Frames of the planet game with its screen drawn over the world, as the player sees it (G2.11
//! A3): `creeer_client --planet <view> <file.png> --hud [--size 1920x1080] [--ui-scale 1.5]
//! [--menu <name>]`. The screen is drawn by the game's own function (`hud.rs`), so the frame
//! shows what the game shows.
//!
//! Views of its own, each in first person as the game is played:
//! - `aim`: empty hands, looking at a pole lying within reach;
//! - `aim-far`: the same pole a few metres off, out of reach;
//! - `heavy`: looking at a 200 kg crate within reach;
//! - `work`: a rail held on a sill, J pressed half a second before (the ring fills).

use crate::planet_shot_build::play;
use crate::planet_view::PlanetView;
use creeer_core::TimingStats;
use creeer_math::glam::DVec3;
use creeer_net::message::Command;
use creeer_render::{Camera, OverlayFrame};
use creeer_ui::{PerfInfo, egui};

/// The views this module takes.
pub(crate) const VIEWS: [&str; 4] = ["aim", "aim-far", "heavy", "work"];

/// What `--hud` asks of a frame.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HudShot {
    /// The image's size (pixels).
    pub size: (u32, u32),
    /// The interface's scale (the settings' UI scale times the screen's own).
    pub scale: f32,
    /// A menu or window to show open.
    pub menu: Option<String>,
    /// Frames to time the screen over (`--hud-frames`), none: not timed.
    pub frames: usize,
}

impl HudShot {
    /// From the command line: `None` without `--hud`.
    ///
    /// # Errors
    /// If a value is not understood.
    pub(crate) fn parse(args: &[String]) -> Result<Option<Self>, String> {
        if !args.iter().any(|a| a == "--hud") {
            return Ok(None);
        }
        let value = |flag: &str| {
            let i = args.iter().position(|a| a == flag)?;
            args.get(i + 1).map(String::as_str)
        };
        let size = match value("--size") {
            None => (1600, 900),
            Some(s) => {
                let (w, h) = s
                    .split_once('x')
                    .ok_or("--size expects WxH, like 1920x1080")?;
                let w = w.parse().map_err(|_| "--size: not a width")?;
                let h = h.parse().map_err(|_| "--size: not a height")?;
                (w, h)
            }
        };
        let scale = match value("--ui-scale") {
            None => 1.0,
            Some(s) => s.parse().map_err(|_| "--ui-scale expects a number")?,
        };
        let frames = match value("--hud-frames") {
            None => 0,
            Some(s) => s.parse().map_err(|_| "--hud-frames expects a count")?,
        };
        Ok(Some(Self {
            size,
            scale,
            menu: value("--menu").map(str::to_owned),
            frames,
        }))
    }
}

/// The screen drawn, ready for the renderer.
pub(crate) struct Overlay {
    primitives: Vec<egui::ClippedPrimitive>,
    textures: egui::TexturesDelta,
    pixels_per_point: f32,
}

impl Overlay {
    /// As the renderer takes it.
    pub(crate) fn frame(&mut self) -> OverlayFrame<'_> {
        OverlayFrame {
            primitives: &self.primitives,
            textures_delta: &mut self.textures,
            pixels_per_point: self.pixels_per_point,
        }
    }
}

/// Draws the game's screen over `view` seen from `camera`, as `shot` asks.
pub(crate) fn overlay(shot: &HudShot, view: &mut PlanetView, camera: &Camera) -> Overlay {
    let ctx = egui::Context::default();
    ctx.set_zoom_factor(shot.scale);
    // As the game does while the player walks: the crosshair aims from the avatar.
    view.walking = true;
    if shot.menu.as_deref() == Some("inventory") {
        view.inventory = true;
    }
    let (frames, ticks) = (TimingStats::new(1), TimingStats::new(1));
    let mut profiler = crate::profiler_ui::ProfilerWindow::default();
    // One frame of the screen at `time` (s).
    let mut run = |time: f64| {
        let mut input = raw_input(shot);
        input.time = Some(time);
        ctx.run_ui(input, |ui| {
            let perf = PerfInfo {
                frame_times: &frames,
                tick_times: &ticks,
                tick_rate_hz: crate::TICK_RATE_HZ,
                tick: 0,
                adapter: "headless",
            };
            let bindings = crate::bindings::Bindings::default();
            crate::hud::draw(ui.ctx(), view, camera, (&perf, &mut profiler), &bindings);
        })
    };
    let mut textures = egui::TexturesDelta::default();
    let (mut shapes, mut pixels_per_point) = (Vec::new(), shot.scale);
    // Windows are measured in their first frame and drawn from the second, and fade in over a
    // few tenths of a second: three frames a second apart.
    for k in 0..3 {
        let output = run(f64::from(k));
        textures.append(output.textures_delta);
        shapes = output.shapes;
        pixels_per_point = output.pixels_per_point;
    }
    // `--hud-frames n`: what the screen costs a frame (laid out and turned into triangles),
    // the world standing still.
    if shot.frames > 0 {
        let mut times = TimingStats::new(shot.frames);
        for k in 0..shot.frames {
            let started = std::time::Instant::now();
            #[allow(clippy::cast_precision_loss)] // frame counts
            let output = run(3.0 + k as f64 / 60.0);
            let triangles = ctx.tessellate(output.shapes, output.pixels_per_point);
            times.push(started.elapsed());
            std::hint::black_box(triangles);
        }
        let p = |q| times.percentile_ms(q).unwrap_or(f64::NAN);
        println!(
            "hud: {} frames, p50 {:.3} ms, p99 {:.3} ms, max {:.3} ms",
            shot.frames,
            p(50.0),
            p(99.0),
            times.max_ms().unwrap_or(f64::NAN)
        );
    }
    Overlay {
        primitives: ctx.tessellate(shapes, pixels_per_point),
        textures,
        pixels_per_point,
    }
}

/// A frame's input: the screen, at one physical pixel per point before the UI scale.
fn raw_input(shot: &HudShot) -> egui::RawInput {
    #[allow(clippy::cast_precision_loss)] // screen sizes
    let size = egui::vec2(shot.size.0 as f32, shot.size.1 as f32);
    let mut input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            size / shot.scale,
        )),
        ..egui::RawInput::default()
    };
    let info = input.viewports.entry(egui::ViewportId::ROOT).or_default();
    info.native_pixels_per_point = Some(1.0);
    input
}

/// In this module's views (first person) the player's own body is not drawn: the eyes are in it.
pub(crate) fn not_own(view: &PlanetView, name: &str, people: &mut Vec<crate::people::DrawnPerson>) {
    let own = (view.session.connection.welcome.as_ref()).map(|w| w.avatar.0);
    if VIEWS.contains(&name) {
        people.retain(|p| Some(p.key) != own);
    }
}

/// The kit's item named `name` (its index, as `Command::Kit` takes it).
fn kit_item(name: &str) -> Result<u8, String> {
    let kit = creeer_sim::items::kit::kit(&creeer_data::Data::base())?;
    let k = (kit.iter().position(|k| k.name == name)).ok_or(format!("no {name} in the kit"))?;
    u8::try_from(k).map_err(|e| e.to_string())
}

/// The camera at the player's eyes looking at `target`.
fn looking_at(view: &PlanetView, target: DVec3) -> Result<Camera, String> {
    let me = view
        .session
        .connection
        .avatar()
        .ok_or("no avatar")?
        .position;
    let eye = creeer_sim::reach::eye(me, me.normalize());
    Ok(crate::planet_camera::place(eye, (target - eye).normalize()))
}

/// The camera of view `name` (one of [`VIEWS`]), the scene built and played to it.
///
/// # Errors
/// If the player has not come, or a part was not put down.
pub(crate) fn camera(view: &mut PlanetView, name: &str) -> Result<Camera, String> {
    view.walking = true;
    if name == "work" {
        let camera = crate::planet_shot_build::camera(view, "rail")?;
        // A lashing: eight seconds, the ring a while filling.
        let names = &view.fastening.names;
        if let Some(k) = names.iter().position(|n| n.contains("lashing")) {
            view.fastening.chosen = k;
        }
        view.fasten(&camera);
        play(view, 0.6)?;
        return Ok(camera);
    }
    play(view, 2.0)?;
    let me = (view.session.connection.avatar())
        .ok_or("no avatar")?
        .position;
    let up = me.normalize();
    let (_, north) = creeer_terrain::geology::deposits::tangent_frame(up);
    let terrain = std::sync::Arc::clone(&view.terrain);
    let ground =
        |p: DVec3| p.normalize() * (terrain.planet().datum.radius(p) + terrain.surface_height(p));
    let (off, command) = match name {
        "heavy" => (1.8, Command::Crate(ground(me + north * 1.8) + up * 0.6)),
        _ => {
            let off = if name == "aim-far" { 4.5 } else { 1.3 };
            let at = ground(me + north * off) + up * 0.15;
            let item = kit_item("pole")?;
            (off, Command::Kit { item, at })
        }
    };
    view.send(command);
    play(view, 2.0)?;
    let at = ground(me + north * off);
    // Where it lies as drawn: the body nearest where it was put down.
    let c = &view.session.connection;
    let own = c.welcome.as_ref().map(|w| w.avatar);
    let lies = (c.poses().into_iter())
        .filter(|(id, _)| Some(*id) != own)
        .map(|(_, p)| p.position)
        .min_by(|a, b| (*a - at).length().total_cmp(&(*b - at).length()))
        .ok_or("the part is not there")?;
    looking_at(view, lies)
}
