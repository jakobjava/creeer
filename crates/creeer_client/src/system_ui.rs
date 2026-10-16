//! The solar-system sandbox's windows: time and warp, the frame, ships and their engine,
//! orbits in numbers, the bodies (and pushing them), the log, and the map's labels.

use crate::system_cmd::{Command, Direction};
use crate::system_map::Label;
use crate::system_sim::SystemSnapshot;
use crate::system_view::SystemView;
use creeer_math::det;
use creeer_math::glam::DVec3;
use creeer_orbit::analysis;
use creeer_orbit::kepler::Elements;
use creeer_render::Camera;
use creeer_sim::universe::terrains::BodyRef;
use creeer_ui::egui;

/// What the window asks the app to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    /// Nothing.
    None,
    /// Leave the sandbox.
    Leave,
}

/// Warp steps offered.
pub const WARPS: [f64; 10] = [1.0, 2.0, 4.0, 10.0, 100.0, 1e3, 1e4, 1e5, 1e6, 1e7];

/// "day 3, 04:12:09"
pub fn clock(t: f64) -> String {
    let day = (t / 86_400.0).floor();
    let rest = t - day * 86_400.0;
    let (h, m, s) = (
        (rest / 3600.0).floor(),
        ((rest % 3600.0) / 60.0).floor(),
        rest % 60.0,
    );
    format!("day {day:.0}, {h:02.0}:{m:02.0}:{s:02.0}")
}

/// A distance in km, or Mm/Gm when large.
pub fn distance(m: f64) -> String {
    match m.abs() {
        d if d >= 1e9 => format!("{:.2} Gm", m / 1e9),
        d if d >= 1e7 => format!("{:.0} km", m / 1e3),
        _ => format!("{:.1} km", m / 1e3),
    }
}

/// A duration in s, min, h or days.
pub fn duration(s: f64) -> String {
    match s {
        s if !s.is_finite() => "∞".into(),
        s if s >= 2.0 * 86_400.0 => format!("{:.1} d", s / 86_400.0),
        s if s >= 2.0 * 3600.0 => format!("{:.1} h", s / 3600.0),
        s if s >= 120.0 => format!("{:.1} min", s / 60.0),
        s => format!("{s:.0} s"),
    }
}

fn orbit_text(ui: &mut egui::Ui, el: &Elements, radius: f64) {
    ui.label(format!(
        "periapsis {} · apoapsis {} (altitudes)",
        distance(el.periapsis() - radius),
        if el.e < 1.0 {
            distance(el.apoapsis() - radius)
        } else {
            "escape".into()
        }
    ));
    ui.label(format!(
        "e {:.4} · i {:.2}° · period {}",
        el.e,
        el.inclination.to_degrees(),
        duration(el.period())
    ));
}

/// The main window.
pub fn window(
    ctx: &egui::Context,
    view: &mut SystemView,
    s: &SystemSnapshot,
    camera: &Camera,
) -> Request {
    let mut request = Request::None;
    let name = |r: BodyRef| {
        s.bodies
            .iter()
            .find(|b| b.which == r)
            .map_or_else(|| "?".into(), |b| b.body.name.clone())
    };
    egui::Window::new("Solar system (F4)")
        .default_pos([12.0, 330.0])
        .resizable(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("◂ tech demo").clicked() {
                    request = Request::Leave;
                }
                let label = if view.map {
                    "flight view (M)"
                } else {
                    "map (M)"
                };
                if ui.button(label).clicked() {
                    view.map = !view.map;
                }
            });
            let warp = if s.warp < s.requested_warp {
                format!(
                    "{:.3e}× (asked {:.0e}×: limited by the tick budget)",
                    s.warp, s.requested_warp
                )
            } else {
                format!("{}×", s.warp)
            };
            ui.label(format!(
                "{} · warp {warp} · tick {:.2} ms",
                clock(s.time),
                s.step_ms
            ));
            ui.horizontal_wrapped(|ui| {
                for w in WARPS {
                    if ui
                        .selectable_label((s.requested_warp - w).abs() < 1e-9 * w, format!("{w}×"))
                        .clicked()
                    {
                        view.send(Command::Warp(w));
                    }
                }
            });
            ui.label(match s.anchor {
                Some(b) => format!("frame: turning with {}", name(b)),
                None => "frame: free fall (floating origin)".into(),
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.add(
                    egui::Slider::new(&mut view.ship_altitude, 20e3..=5e6)
                        .logarithmic(true)
                        .text("m"),
                );
                if ui.button("ship in orbit").clicked() {
                    view.send(Command::ShipInOrbit(view.ship_altitude));
                    view.follow = Some(DVec3::new(0.0, 6.0, 25.0));
                }
            });
            ui.horizontal(|ui| {
                let ahead = camera.position + camera.forward() * 12.0;
                if ui.button("crate").clicked() {
                    view.send(Command::Crate(ahead));
                }
                if ui.button("ball").clicked() {
                    view.send(Command::Ball(ahead));
                }
                if ui.button("clear").clicked() {
                    view.send(Command::Clear);
                    view.follow = None;
                }
                let mut following = view.follow.is_some();
                if ui.checkbox(&mut following, "camera follows ship").changed() {
                    view.follow = following.then_some(DVec3::new(0.0, 6.0, 25.0));
                }
            });
            if let Some(f) = &s.focus {
                ui.separator();
                let primary = &s.bodies[f.primary].body;
                ui.label(format!(
                    "{} (#{}, {:.0} t) around {}: altitude {}, speed {:.1} m/s",
                    f.name,
                    f.id,
                    f.mass / 1e3,
                    primary.name,
                    distance(f.altitude),
                    f.speed
                ));
                orbit_text(ui, &f.elements, primary.radius);
                let mut engine = view.engine;
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Slider::new(&mut engine.thrust, 1e3..=1e7)
                            .logarithmic(true)
                            .text("N"),
                    );
                    ui.label(format!("{:.2} m/s²", engine.thrust / f.mass));
                });
                ui.horizontal_wrapped(|ui| {
                    for (d, label) in Direction::ALL {
                        ui.radio_value(&mut engine.direction, d, label);
                    }
                });
                ui.checkbox(&mut engine.on, "engine burning (B)");
                if engine != view.engine {
                    view.engine = engine;
                    view.send(Command::Engine(engine));
                }
                if let Some(impact) = s.prediction.1.impact {
                    ui.colored_label(
                        egui::Color32::LIGHT_RED,
                        format!("on course to hit {}", name(impact)),
                    );
                }
            }
            ui.separator();
            ui.checkbox(
                &mut view.corotating,
                "map turns with the body pair (Lagrange points)",
            );
        });
    bodies_window(ctx, view, s);
    egui::Window::new("Log")
        .default_pos([12.0, 820.0])
        .resizable(false)
        .show(ctx, |ui| {
            for line in s.log.iter() {
                ui.label(line);
            }
        });
    request
}

fn bodies_window(ctx: &egui::Context, view: &mut SystemView, s: &SystemSnapshot) {
    egui::Window::new("Bodies")
        .default_pos([1250.0, 12.0])
        .default_height(420.0)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .max_height(220.0)
                .show(ui, |ui| {
                    for b in s
                        .bodies
                        .iter()
                        .filter(|b| matches!(b.which, BodyRef::Major(_)))
                    {
                        let depth = if b.parent.is_some_and(|p| p != 0) {
                            "    "
                        } else {
                            ""
                        };
                        let text = format!("{depth}{} ({:?})", b.body.name, b.body.kind);
                        if ui
                            .selectable_label(view.selected == Some(b.which), text)
                            .clicked()
                        {
                            view.selected = Some(b.which);
                        }
                    }
                    ui.label(format!(
                        "+ {} small bodies on rails",
                        s.bodies
                            .iter()
                            .filter(|b| matches!(b.which, BodyRef::Minor(_)))
                            .count()
                    ));
                });
            let Some(selected) = view.selected else {
                return;
            };
            let Some(b) = s.bodies.iter().find(|b| b.which == selected) else {
                return;
            };
            ui.separator();
            let body = &b.body;
            ui.label(format!(
                "{}: radius {}, g {:.3} m/s², density {:.0} kg/m³",
                body.name,
                distance(body.radius),
                body.surface_gravity(),
                body.density()
            ));
            ui.label(format!(
                "mass {:.3e} kg, day {}",
                body.mass(),
                duration(std::f64::consts::TAU / body.spin.rate)
            ));
            if let Some(parent) = b.parent.and_then(|p| s.bodies.get(p)) {
                let mu = parent.body.mu + body.mu;
                let el = Elements::from_state(
                    mu,
                    b.state.0 - parent.state.0,
                    b.state.1 - parent.state.1,
                );
                ui.label(format!(
                    "around {}: a {}, Hill radius {}",
                    parent.body.name,
                    distance(el.semi_major_axis()),
                    distance(analysis::hill_radius(
                        el.semi_major_axis(),
                        el.e,
                        body.mu,
                        parent.body.mu
                    ))
                ));
                orbit_text(ui, &el, 0.0);
            }
            ui.separator();
            ui.label("Push it (an engine, a mass driver):");
            ui.add(egui::Slider::new(&mut view.push.0, 3.0..=20.0).text("log₁₀ force (N)"));
            ui.horizontal_wrapped(|ui| {
                for (d, label) in Direction::ALL {
                    ui.radio_value(&mut view.push.1, d, label);
                }
            });
            ui.add(
                egui::Slider::new(&mut view.push.2, 1.0..=1e7)
                    .logarithmic(true)
                    .text("s"),
            );
            let force = det::pow(10.0, view.push.0);
            let dv = force * view.push.2 / body.mass();
            ui.label(format!(
                "{force:.2e} N for {}: Δv {dv:.3e} m/s",
                duration(view.push.2)
            ));
            if ui.button("push").clicked() {
                view.send(Command::Push(selected, force, view.push.1, view.push.2));
            }
        });
}

/// Draws map labels at their projected screen positions.
pub fn paint_labels(ctx: &egui::Context, camera: &Camera, labels: &[Label]) {
    let rect = ctx.content_rect();
    let aspect = rect.width() / rect.height().max(1.0);
    let view_proj = camera.view_projection(aspect);
    let painter = ctx.layer_painter(egui::LayerId::background());
    for label in labels {
        let rel = (label.at - camera.position).as_vec3();
        let clip = view_proj * rel.extend(1.0);
        if clip.w <= 0.0 {
            continue;
        }
        let (x, y) = (clip.x / clip.w, clip.y / clip.w);
        if x.abs() > 1.2 || y.abs() > 1.2 {
            continue;
        }
        let at = egui::pos2(
            rect.left() + (x + 1.0) * 0.5 * rect.width(),
            rect.top() + (1.0 - y) * 0.5 * rect.height(),
        );
        let [r, g, b] = label.color;
        let color = egui::Color32::from_rgb(r, g, b);
        painter.circle_stroke(at, 4.0, egui::Stroke::new(1.5, color));
        painter.text(
            at + egui::vec2(7.0, -7.0),
            egui::Align2::LEFT_BOTTOM,
            &label.text,
            egui::FontId::proportional(13.0),
            color,
        );
    }
}

#[cfg(test)]
#[path = "system_ui_tests.rs"]
mod tests;
