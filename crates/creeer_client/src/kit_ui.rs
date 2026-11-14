//! The creative kit as a palette (C; G2.11 A3, ADR-0038): every part of `data/kit.ron` with its
//! name, what it is made of, its size and weight, and a button that puts it down where the
//! crosshair is within reach (on what is there, as a crane sets a load), else on the ground a
//! step before the player, to be taken up by hand (G). A catalogue of parts, as a timber yard
//! lists them: nothing is placed for the player, no ghost, no snapping (plan.md, principle 13).
//! A creative server's alone; a survival one refuses, and says so.

use crate::planet_view::PlanetView;
use creeer_math::glam::DVec3;
use creeer_net::message::Command;
use creeer_render::Camera;
use creeer_sim::items::{Form, Item};
use creeer_ui::egui::{self, RichText};
use creeer_ui::theme::TEXT_DIM;
use std::sync::OnceLock;

/// A part of the kit as the palette shows it, its half height as it is put down (m), the edges
/// of the box it fits in (m, longest first), and whether it is a container.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct KitPart {
    pub name: String,
    pub material: String,
    pub size: String,
    pub mass: f64,
    pub half_up: f64,
    pub extents: [f64; 3],
    pub container: bool,
}

/// The kit's part a thing is, known by its size and weight as a person at the timber yard knows
/// a stringer from a deck board (`extents`: its box's edges, longest first, m; within 2 mm, and
/// within 3 % of its weight but for a container, whatever it holds). The crosshair names it so
/// (G2.11 Q: every sawn part was a "beam").
pub(crate) fn by_shape(extents: [f64; 3], mass: f64) -> Option<&'static KitPart> {
    parts().iter().find(|p| {
        let near = (p.extents.iter().zip(extents)).all(|(a, b)| (a - b).abs() < 0.002);
        near && (p.container || (p.mass - mass).abs() <= 0.03 * p.mass)
    })
}

/// The kit's parts, in its order (by which `Command::Kit` names them).
pub(crate) fn parts() -> &'static [KitPart] {
    static PARTS: OnceLock<Vec<KitPart>> = OnceLock::new();
    PARTS.get_or_init(|| {
        let data = creeer_data::Data::base();
        let kit = creeer_sim::items::kit::kit(&data).unwrap_or_default();
        let densities = creeer_sim::items::kit::Densities::from_data(&data).ok();
        let materials = creeer_materials::MaterialTable::from_data(&data).ok();
        (kit.iter())
            .map(|k| {
                let item = densities.as_ref().and_then(|d| d.make(k).ok());
                let (size, mass, half_up, extents) = match &item {
                    Some(item @ Item::Piece(p, _)) => (
                        size_words(&p.form),
                        item.mass(),
                        half_up(&p.form),
                        p.form.extents(),
                    ),
                    Some(item) => (String::new(), item.mass(), 0.1, [0.0; 3]),
                    None => (String::new(), 0.0, 0.1, [0.0; 3]),
                };
                let material = (materials.as_ref())
                    .and_then(|m| m.get(&k.material).map(|m| short(&m.name)))
                    .unwrap_or_else(|| k.material.replace('_', " "));
                KitPart {
                    name: k.name.clone(),
                    material,
                    size,
                    mass,
                    half_up,
                    extents,
                    container: k.wall > 0.0,
                }
            })
            .collect()
    })
}

/// A shape's half height as the server puts it down (upright: a block's second size up, a rod
/// lying), m.
fn half_up(form: &Form) -> f64 {
    match *form {
        Form::Rod { radius, .. } | Form::Ball { radius } => radius,
        Form::Block { size } | Form::Lump { size } => size.y * 0.5,
    }
}

/// A material's name up to its notes in brackets ("Spruce, green (Norway spruce; …)").
fn short(name: &str) -> String {
    name.split(" (").next().unwrap_or(name).trim().to_owned()
}

/// A shape's size in words, as a catalogue gives it.
pub(crate) fn size_words(form: &Form) -> String {
    let length = |m: f64| {
        if m >= 1.0 {
            format!("{m:.1} m")
        } else {
            format!("{:.0} cm", m * 100.0)
        }
    };
    match *form {
        Form::Rod { radius, length: l } => {
            format!("a rod {} long, {:.0} mm thick", length(l), radius * 2000.0)
        }
        Form::Ball { radius } => format!("a ball {} across", length(2.0 * radius)),
        Form::Block { .. } | Form::Lump { .. } => {
            let [a, b, c] = form.extents();
            format!("{} × {} × {}", length(a), length(b), length(c))
        }
    }
}

/// A weight as a catalogue gives it: grams under a kilogram.
fn weight(kg: f64) -> String {
    if kg < 1.0 {
        format!("{:.0} g", kg * 1000.0)
    } else {
        format!("{kg:.1} kg")
    }
}

/// How far before the player a part is put down when the crosshair is on nothing within reach
/// (m), and how far above what it rests on (m: set down, not dropped: a 1 t weight dropped from
/// 0.3 m onto a deck broke it, ADR-0039).
const AHEAD: f64 = 1.3;
const CLEAR: f64 = 0.01;

/// Where a part `half_up` m half high is put down: where the crosshair meets something within
/// the hands' reach, else on the ground 1.3 m before the player; there on top of whatever is
/// under that spot (a deck, a crate, the ground), a centimetre clear of it.
fn where_put(view: &PlanetView, camera: &Camera, half_up: f64) -> Option<DVec3> {
    let c = &view.session.connection;
    let me = c.avatar()?.position;
    let up = me.normalize();
    let eye = creeer_sim::reach::eye(me, up);
    let aimed = (c.aim(
        camera.position,
        camera.forward(),
        2.0 * creeer_sim::reach::PLACING,
    ))
    .filter(|a| (a.point - eye).length() <= creeer_sim::reach::PLACING);
    let ahead = camera.forward().reject_from(up).normalize_or_zero();
    let spot = aimed.map_or(me + ahead * AHEAD, |a| a.point);
    // The top of what is under the spot, from above whatever is there.
    let above = spot + up * (2.0 * half_up + 1.0);
    let rest = c.aim(above, -up, 2.0 * half_up + 4.0)?.point;
    Some(rest + up * (half_up + CLEAR))
}

/// The palette's window, while `open`: at the right below what failed (told at the top), as tall
/// as the screen leaves above what the hands hold (its list scrolls). `take` names the key that
/// takes a part up (the controls' table).
pub(crate) fn window(
    ctx: &egui::Context,
    open: &mut bool,
    (view, camera): (&mut PlanetView, &Camera),
    take: &str,
) {
    let mut asked = None;
    let tall = (ctx.content_rect().height() - TOP - BELOW - FRAME).max(160.0);
    egui::Window::new("Creative kit")
        .open(open)
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::RIGHT_TOP, [-16.0, TOP])
        .show(ctx, |ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            let hint = format!("Put down where you look, or before you; then take it up ({take}).");
            ui.label(RichText::new(hint).color(TEXT_DIM));
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(tall)
                .min_scrolled_height(tall)
                .show(ui, |ui| rows(ui, &mut asked));
        });
    if let Some(item) = asked {
        put_down(view, camera, item);
    }
}

/// The window's top below the screen's top (below the first start's hint and what failed), the
/// room left under it for the hands' panel, and its title's and hint's (points).
const TOP: f32 = 132.0;
const BELOW: f32 = 200.0;
const FRAME: f32 = 80.0;

/// A row a part: its name, size, weight and material, and its button.
fn rows(ui: &mut egui::Ui, asked: &mut Option<u8>) {
    egui::Grid::new("kit")
        .num_columns(5)
        .spacing([12.0, 6.0])
        .striped(true)
        .show(ui, |ui| {
            for (k, part) in parts().iter().enumerate() {
                ui.label(RichText::new(&part.name).strong());
                ui.label(&part.size);
                ui.label(RichText::new(weight(part.mass)).color(TEXT_DIM));
                ui.label(RichText::new(&part.material).color(TEXT_DIM));
                if ui.button("Put down").clicked() {
                    *asked = u8::try_from(k).ok();
                }
                ui.end_row();
            }
        });
}

/// The kit's part `item` put down (its "Put down" button; the played frames press it too,
/// `build_play_site.rs`).
pub(crate) fn put_down(view: &mut PlanetView, camera: &Camera, item: u8) {
    let half_up = parts().get(usize::from(item)).map_or(0.1, |p| p.half_up);
    if let Some(at) = where_put(view, camera, half_up) {
        view.send(Command::Kit { item, at });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_as_a_catalogue_gives_them() {
        let post = Form::Rod {
            radius: 0.05,
            length: 1.6,
        };
        assert_eq!(size_words(&post), "a rod 1.6 m long, 100 mm thick");
        let sill = Form::Block {
            size: DVec3::new(0.15, 2.4, 0.15),
        };
        assert_eq!(size_words(&sill), "2.4 m × 15 cm × 15 cm");
        assert_eq!(
            size_words(&Form::Ball { radius: 0.04 }),
            "a ball 8 cm across"
        );
        assert_eq!(weight(0.04), "40 g");
        assert_eq!(weight(10.05), "10.1 kg");
        assert_eq!(
            short("Spruce, green (Norway spruce; red spruce's values)"),
            "Spruce, green"
        );
    }

    #[test]
    fn the_kit_of_the_base_pack_is_listed_with_sizes_and_weights() {
        let parts = parts();
        let post = parts
            .iter()
            .find(|p| p.name == "post")
            .expect("a post in the kit");
        assert_eq!(post.size, "a rod 1.6 m long, 100 mm thick");
        assert_eq!(post.material, "Spruce, green");
        assert!((5.0..20.0).contains(&post.mass), "{}", post.mass);
        assert!(parts.iter().all(|p| !p.size.is_empty() && p.mass > 0.0));
        // Set down by its half height as the server puts it (upright; a rod lying).
        let half = |name: &str| parts.iter().find(|p| p.name == name).map(|p| p.half_up);
        assert_eq!(half("deck board"), Some(0.025));
        assert_eq!(half("test weight"), Some(0.3));
        assert_eq!(half("post"), Some(0.05));
    }
}
