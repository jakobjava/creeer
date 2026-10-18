//! The planet sandbox's window: tools, walking or flying, time of day, a geology probe at the
//! crosshair, saving and loading the world (when this process hosts it), and live numbers.

use crate::planet_view::{PlanetView, TOOL_REACH, Tool};
use creeer_net::message::Command;
use creeer_render::Camera;
use creeer_ui::egui;
use std::path::Path;

/// Where the sandbox saves its world.
const WORLD_FILE: &str = "planet_world.creeer";
/// Where the F3 sandbox saved its terrain edits (a version 1 save: loaded if there is no
/// world file).
const EDITS_FILE: &str = "planet_edits.bin";

/// What the window asks the app to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    /// Nothing.
    None,
    /// Leave the planet sandbox.
    Leave,
    /// Jump to the orbit view.
    Orbit,
}

/// Draws the window; returns what the app should do.
pub fn window(ctx: &egui::Context, view: &mut PlanetView, camera: &Camera) -> Request {
    let mut request = Request::None;
    let snapshot = view.snapshot();
    egui::Window::new("Planet")
        .default_pos([12.0, 330.0])
        .resizable(false)
        .show(ctx, |ui| {
            let planet = view.terrain.planet();
            ui.label(format!(
                "{}: radius {:.0} km, g {:.2} m/s², day {:.1} h",
                planet.name,
                planet.radius / 1e3,
                planet.mu / (planet.radius * planet.radius),
                planet.sidereal_day / 3600.0
            ));
            ui.horizontal(|ui| {
                if ui.button("◂ tech demo").clicked() {
                    request = Request::Leave;
                }
                if ui.button("orbit view").clicked() {
                    request = Request::Orbit;
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Left click:");
                for (tool, name) in Tool::ALL {
                    ui.radio_value(&mut view.tool, tool, name);
                }
            });
            if view.tool == Tool::Fill {
                let rocks = &view.terrain.planet().geology.rocks.rocks;
                let current = rocks
                    .get(usize::from(view.fill_rock))
                    .map_or("?", |r| r.name.as_str());
                egui::ComboBox::from_label("fill with")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for (i, rock) in rocks.iter().enumerate() {
                            let id = u16::try_from(i).unwrap_or(0);
                            ui.selectable_value(&mut view.fill_rock, id, &rock.name);
                        }
                    });
            }
            ui.horizontal(|ui| {
                ui.checkbox(&mut view.walking, "walk (V)");
                if view.session.hosted() {
                    if ui.button("save world").clicked()
                        && let Err(e) = view.session.save(Path::new(WORLD_FILE))
                    {
                        eprintln!("{e}");
                    }
                    if ui.button("load world").clicked() {
                        let file = if Path::new(WORLD_FILE).exists() {
                            WORLD_FILE
                        } else {
                            EDITS_FILE
                        };
                        if let Err(e) = view.session.load(Path::new(file)) {
                            eprintln!("{e}");
                        }
                    }
                }
                if ui.button("remove bodies").clicked() {
                    view.send(Command::Clear);
                }
            });
            machines(ui, view, camera);
            let edits = view.session.connection.edits.len();
            if view.session.hosted() && Path::new(WORLD_FILE).exists() {
                ui.small(format!("{edits} edits · saved world: {WORLD_FILE}"));
            } else {
                ui.small(format!("{edits} edits"));
            }
            ui.separator();
            let hours = view.day_fraction * view.terrain.planet().sidereal_day / 3600.0;
            ui.add(
                egui::Slider::new(&mut view.day_fraction, 0.0..=1.0)
                    .text(format!("time of day ({hours:.1} h)")),
            );
            ui.horizontal(|ui| {
                ui.label("sun speed:");
                for speed in [0.0, 1.0, 60.0, 600.0] {
                    ui.radio_value(&mut view.day_speed, speed, format!("{speed}×"));
                }
            });
            ui.separator();
            probe(ui, view, camera);
            ui.separator();
            let altitude = view.altitude(camera.position);
            let up = camera.position.normalize();
            let latitude = creeer_math::det::asin(up.y.clamp(-1.0, 1.0)).to_degrees();
            let longitude = creeer_math::det::atan2(up.z, up.x).to_degrees();
            ui.label(format!(
                "altitude {altitude:.1} m · {latitude:.2}° lat, {longitude:.2}° lon"
            ));
            let (bodies, asleep, _) = snapshot.counts;
            let net = view.session.connection.network();
            ui.label(format!(
                "{bodies} in view ({asleep} asleep) · {} · {:.0} ms · ↓{:.0} ↑{:.0} kbit/s",
                if view.session.hosted() {
                    "hosted"
                } else {
                    "online"
                },
                net.rtt * 1e3,
                net.bytes_received_per_second * 8e-3,
                net.bytes_sent_per_second * 8e-3,
            ));
            let (cached, outstanding) = view.streaming();
            ui.label(format!(
                "terrain: {} chunks drawn · {cached} cached · {outstanding} to build",
                view.draw.len()
            ));
            ui.label("Right mouse: look · WASD/Space/Ctrl: fly (speed follows altitude)");
            ui.label("V: walk the avatar (WASD, Space jumps, Shift runs, Ctrl: jetpack)");
            ui.label("Mouse wheel: speed · Left click: use tool at the crosshair");
        });
    request
}

/// What lies under the crosshair: rock, its density, province and any ore deposit.
fn probe(ui: &mut egui::Ui, view: &PlanetView, camera: &Camera) {
    let Some(hit) = view.raycast(camera.position, camera.forward(), TOOL_REACH) else {
        ui.label("probe: nothing within reach");
        return;
    };
    // Half a metre into the ground: the rock that would be dug.
    let below = hit - hit.normalize() * 0.5;
    let terrain = &view.terrain;
    let geology = &terrain.planet().geology;
    let distance = (hit - camera.position).length();
    let Some(rock) = terrain.rock_at(below) else {
        ui.label(format!("probe ({distance:.0} m): air"));
        return;
    };
    let r = &geology.rocks.rocks[usize::from(rock)];
    let sample = geology.sample_at(below, terrain.surface_height(below));
    let province = &geology.provinces[sample.province].name;
    ui.label(format!(
        "probe ({distance:.0} m): {} · {:.0} kg/m³ · {province}",
        r.name, r.density
    ));
    if let Some(hit) = sample.deposit {
        let kind = &geology.deposits[usize::from(hit.kind)].name;
        ui.label(format!(
            "  in a {kind} deposit, grade {:.2}× average",
            hit.grade
        ));
    }
}

/// A small cross at the centre of the screen, where tools act.
pub fn crosshair(ctx: &egui::Context) {
    let center = ctx.content_rect().center();
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("crosshair"),
    ));
    let stroke = egui::Stroke::new(1.5, egui::Color32::from_white_alpha(200));
    for (dx, dy) in [(1.0, 0.0), (0.0, 1.0)] {
        let d = egui::vec2(dx, dy) * 8.0;
        painter.line_segment([center - d, center + d], stroke);
    }
}

/// The world's machines (F7.4): each one's mode (active near a player, abstract beyond
/// 3 km of every player, caught up when one comes near), distance, water and hydrogen.
fn machines(ui: &mut egui::Ui, view: &PlanetView, camera: &Camera) {
    let list = &view.session.connection.machines;
    if list.is_empty() {
        return;
    }
    ui.separator();
    for m in list {
        let mode = match m.mode {
            0 => "active",
            1 => "abstract",
            _ => "catching up",
        };
        let distance = (m.at - camera.position).length();
        ui.small(format!(
            "powerhouse {} · {mode} · {:.1} km · tank {:.1} m³ · H₂ {:.1} mol · {:.0} min",
            m.id,
            distance / 1e3,
            m.water,
            m.hydrogen,
            m.time / 60.0
        ));
    }
}
