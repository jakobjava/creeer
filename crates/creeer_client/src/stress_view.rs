//! What a creative server shows of what is built (G2.11, ADR-0039; plan.md principle 13: debug
//! views are a creative or test server's information, never a survival tool).
//!
//! - **The stress overlay** (O): every built member coloured by its share of what breaks it,
//!   from blue (unloaded) through grey (half) to red (breaking), and every joint as a ball in the
//!   same colours: nails, pegs and lashings are where timber structures fail.
//! - **What failed:** where a member or joint broke, a red mark, and in words what it was and
//!   what it carried ("the joint between a stringer and a deck board pulled apart: 1.9 kN of
//!   pull, 0.3 kN across"); the first of a collapse first, for a minute.
//!
//! Both come from the server this process hosts (`Host::inspection`): on a survival server, or
//! another's, nothing shows. In survival a structure shows only what a real one does: it sags,
//! sways and breaks.

use crate::demo::DemoMeshes;
use creeer_math::Frame64;
use creeer_math::glam::{DQuat, DVec3, Vec3};
use creeer_net::entity::Kind;
use creeer_render::{Camera, Heatmap, Instance, Material, Palette, Scene};
use creeer_server::Connection;
use creeer_server::world::site::{COLLAPSE, Failure, Inspection};
use creeer_ui::egui;

/// The overlay's scale: a share of what breaks it (%), blue to red through grey, as Poly Bridge's
/// stress view reads red as near failing (viridis' dark purple for a lightly loaded member did
/// not read on brown ground).
pub const SCALE: Heatmap = Heatmap {
    min: 0.0,
    max: 100.0,
    palette: Palette::Diverging,
};
/// A joint's ball (m across), and a failure's mark.
const JOINT: f64 = 0.1;
const MARK: f64 = 0.09;
/// How long a failure stays marked (ticks: a minute).
const SHOWN: u64 = 3600;
const TIMBER: Material = Material::dielectric([0.62, 0.46, 0.28], 0.85);
const FAILED: Material = Material::emissive([1.0, 0.08, 0.04], 3.0);

/// Whether the overlay is on.
#[derive(Clone, Copy, Debug, Default)]
pub struct StressView {
    pub on: bool,
}

impl StressView {
    /// The overlay's scale, while it is on.
    pub fn heatmap(self) -> Option<Heatmap> {
        self.on.then_some(SCALE)
    }

    /// While on, every structure (the caller leaves them out of its own drawing: `leave_out`),
    /// the built ones coloured; and the failures' marks.
    pub fn draw(self, scene: &mut Scene, meshes: &DemoMeshes, c: &Connection, seen: &Inspection) {
        let poses = c.poses();
        for (id, kind) in c.kinds.iter().filter(|_| self.on) {
            let (Kind::Structure(frame), Some(pose)) = (kind, poses.get(id)) else {
                continue;
            };
            let none = Vec::new();
            let shares = (seen.shares.iter())
                .find(|(s, _)| s == id)
                .map_or(&none, |(_, s)| s);
            let at = |n: u32| {
                (frame.nodes.get(n as usize)).map(|v| pose.position + pose.orientation * *v)
            };
            for (k, [a, b]) in frame.members.iter().enumerate() {
                let (Some(a), Some(b)) = (at(*a), at(*b)) else {
                    continue;
                };
                let heat = shares.get(k).map(|s| s.clamp(0.0, 1.0) * 100.0);
                let radius = frame.radii.get(k).copied().unwrap_or(frame.radius);
                if radius > 0.0 {
                    rod(scene, meshes, (a, b, radius), heat);
                } else {
                    ball(scene, meshes, (a + b) * 0.5, JOINT, (TIMBER, heat));
                }
            }
        }
        for f in &seen.failures {
            ball(scene, meshes, f.at, MARK, (FAILED, None));
        }
    }

    /// Leaves out of `beams` what the overlay draws itself while it is on: the structures'
    /// members (a planet's snapshot has no other beams).
    pub fn leave_out(self, beams: &mut Vec<(DVec3, DVec3, f64, f32, bool)>) {
        if self.on {
            beams.clear();
        }
    }
}

/// The failures to tell now (`tick`: the server's): each collapse's first, within a minute,
/// the newest first, with how many broke in it.
pub fn told(seen: &Inspection, tick: u64) -> Vec<(&Failure, usize)> {
    let recent: Vec<&Failure> = (seen.failures.iter())
        .filter(|f| tick.saturating_sub(f.tick) < SHOWN)
        .collect();
    // Failures come oldest first: one is a collapse's first if none came within its time before.
    let mut out: Vec<(&Failure, usize)> = Vec::new();
    for (k, f) in recent.iter().enumerate() {
        let first = !recent[..k].iter().any(|g| f.tick - g.tick < COLLAPSE);
        if first {
            let n = (recent[k..].iter())
                .filter(|g| g.tick < f.tick + COLLAPSE)
                .count();
            out.push((f, n));
        }
    }
    out.reverse();
    out
}

/// The words of what failed, beside its mark and at the top of the screen.
pub fn hud(ctx: &egui::Context, camera: &Camera, seen: &Inspection, tick: u64) {
    let told = told(seen, tick);
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("failed"),
    ));
    let screen = ctx.content_rect();
    let red = egui::Color32::from_rgb(255, 120, 100);
    if let Some((first, n)) = told.first() {
        let more = if *n > 1 {
            format!(" (then {} more)", n - 1)
        } else {
            String::new()
        };
        let text = format!("First to fail: {}{more}", first.words);
        let top = egui::pos2(screen.center().x, screen.top() + 60.0);
        label(&painter, (top, egui::Align2::CENTER_TOP), text, 17.0);
    }
    for (f, _) in &told {
        if let Some(at) = on_screen(camera, f.at, screen) {
            painter.circle_stroke(at, 12.0, egui::Stroke::new(2.5, red));
            let beside = (at + egui::vec2(18.0, -8.0), egui::Align2::LEFT_TOP);
            label(&painter, beside, f.words.clone(), 13.0);
        }
    }
}

/// `text` in the failure's red on a dark ground, readable over sky and earth alike.
fn label(
    painter: &egui::Painter,
    (at, align): (egui::Pos2, egui::Align2),
    text: String,
    size: f32,
) {
    let red = egui::Color32::from_rgb(255, 120, 100);
    let galley = painter.layout_no_wrap(text, egui::FontId::proportional(size), red);
    let rect = align.anchor_size(at, galley.size());
    painter.rect_filled(rect.expand(5.0), 4.0, egui::Color32::from_black_alpha(180));
    painter.galley(rect.min, galley, red);
}

/// Where `p` is drawn on `screen`, if before the camera.
fn on_screen(camera: &Camera, p: DVec3, screen: egui::Rect) -> Option<egui::Pos2> {
    let aspect = screen.width() / screen.height().max(1.0);
    let relative = (p - camera.position).as_vec3();
    let clip = camera.view_projection(aspect) * relative.extend(1.0);
    (clip.w > 0.0).then(|| {
        let (x, y) = (clip.x / clip.w, clip.y / clip.w);
        egui::pos2(
            screen.left() + (x + 1.0) * 0.5 * screen.width(),
            screen.top() + (1.0 - y) * 0.5 * screen.height(),
        )
    })
}

fn rod(
    scene: &mut Scene,
    meshes: &DemoMeshes,
    (a, b, radius): (DVec3, DVec3, f64),
    heat: Option<f32>,
) {
    let along = b - a;
    let length = along.length();
    if length < 1e-9 {
        return;
    }
    #[allow(clippy::cast_possible_truncation)] // render-space sizes
    let scale = Vec3::new(2.0 * radius as f32, length as f32, 2.0 * radius as f32);
    scene.add(
        meshes.cylinder,
        Instance {
            frame: Frame64::new(
                (a + b) * 0.5,
                DQuat::from_rotation_arc(DVec3::Y, along / length),
            ),
            scale,
            material: TIMBER,
            heat,
        },
    );
}

fn ball(
    scene: &mut Scene,
    meshes: &DemoMeshes,
    at: DVec3,
    across: f64,
    (material, heat): (Material, Option<f32>),
) {
    #[allow(clippy::cast_possible_truncation)] // render-space sizes
    let scale = Vec3::splat(across as f32);
    scene.add(
        meshes.sphere,
        Instance {
            frame: Frame64::from_position(at),
            scale,
            material,
            heat,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failure(tick: u64, words: &str) -> Failure {
        Failure {
            tick,
            structure: creeer_net::entity::EntityId(1),
            at: DVec3::ZERO,
            joint: false,
            words: words.into(),
        }
    }

    /// A collapse is told by its first failure with how many followed in it; a later one apart;
    /// none after a minute.
    #[test]
    fn a_collapse_is_told_by_its_first_failure() {
        let seen = Inspection {
            shares: Vec::new(),
            failures: vec![
                failure(100, "first"),
                failure(100, "with it"),
                failure(150, "after it"),
                failure(1000, "later"),
            ],
        };
        let told = told(&seen, 1200);
        let words: Vec<(&str, usize)> = told.iter().map(|(f, n)| (f.words.as_str(), *n)).collect();
        assert_eq!(words, [("later", 1), ("first", 3)]);
        assert!(super::told(&seen, 100 + SHOWN + 1000).is_empty());
    }
}
