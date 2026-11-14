//! The creative kit as a palette (C; G2.11 A3, ADR-0037): every part of `data/kit.ron` with its
//! name, what it is made of, its size and weight, and a button that puts it down on the ground
//! a step before the player, to be taken up by hand (G). A catalogue of parts, as a timber yard
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

/// A part of the kit as the palette shows it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct KitPart {
    pub name: String,
    pub material: String,
    pub size: String,
    pub mass: f64,
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
                let (size, mass) = match &item {
                    Some(item @ Item::Piece(p, _)) => (size_words(&p.form), item.mass()),
                    Some(item) => (String::new(), item.mass()),
                    None => (String::new(), 0.0),
                };
                let material = (materials.as_ref())
                    .and_then(|m| m.get(&k.material).map(|m| short(&m.name)))
                    .unwrap_or_else(|| k.material.replace('_', " "));
                KitPart {
                    name: k.name.clone(),
                    material,
                    size,
                    mass,
                }
            })
            .collect()
    })
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

/// Where a part asked for is put down: on the ground 1.3 m before the player, a little above it.
fn before_the_player(view: &PlanetView, camera: &Camera) -> Option<DVec3> {
    let me = view.session.connection.avatar()?.position;
    let up = me.normalize();
    let ahead = camera.forward().reject_from(up).normalize_or_zero();
    let p = me + ahead * 1.3;
    let planet = view.terrain.planet();
    let ground = p.normalize() * (planet.datum.radius(p) + view.terrain.surface_height(p));
    Some(ground + up * 0.3)
}

/// The palette's window, while `open`.
pub(crate) fn window(ctx: &egui::Context, open: &mut bool, view: &mut PlanetView, camera: &Camera) {
    let mut asked = None;
    egui::Window::new("Creative kit")
        .open(open)
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::RIGHT_CENTER, [-24.0, 0.0])
        .show(ctx, |ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            ui.label(RichText::new("Put down before you, then take it up (G).").color(TEXT_DIM));
            ui.separator();
            egui::Grid::new("kit")
                .num_columns(3)
                .spacing([14.0, 8.0])
                .striped(true)
                .show(ui, |ui| {
                    for (k, part) in parts().iter().enumerate() {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&part.name).strong());
                            ui.label(RichText::new(&part.material).color(TEXT_DIM));
                        });
                        ui.vertical(|ui| {
                            ui.label(&part.size);
                            ui.label(RichText::new(weight(part.mass)).color(TEXT_DIM));
                        });
                        if ui.button("Put down").clicked() {
                            asked = u8::try_from(k).ok();
                        }
                        ui.end_row();
                    }
                });
        });
    if let Some(item) = asked
        && let Some(at) = before_the_player(view, camera)
    {
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
    }
}
