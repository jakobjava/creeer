//! What the player carries (G1.6a, ADR-0028): E puts away what the hands hold or the item the
//! crosshair is on, F puts what the hands hold into the container the crosshair is on (or looks
//! into it), I shows the pockets and the container looked into, each item with what can be seen
//! of it, to take out into the hands. A plain list: the game's own look for it is G11's.

use crate::planet_aim::target;
use crate::planet_view::PlanetView;
use creeer_net::message::{Command, Target};
use creeer_render::Camera;
use creeer_ui::egui;
use std::sync::OnceLock;

/// The creative kit's items' names, in order (`data/kit.ron`).
fn kit() -> &'static [String] {
    static KIT: OnceLock<Vec<String>> = OnceLock::new();
    KIT.get_or_init(|| {
        let data = creeer_data::Data::base();
        (creeer_sim::items::kit::kit(&data)
            .unwrap_or_default()
            .into_iter())
        .map(|k| k.name)
        .collect()
    })
}

impl PlanetView {
    /// What the crosshair is on as a target, if it is a body within reach.
    fn reached(&self, camera: &Camera) -> Option<Target> {
        let t = target(self, camera).filter(|t| t.in_reach && t.aimed.body.is_some())?;
        Some(Target {
            body: t.aimed.body,
            at: t.aimed.point,
            seen: t.aimed.seen,
        })
    }

    /// Puts away what the hands hold, or the item the crosshair is on.
    pub fn stow(&mut self, camera: &Camera) {
        if self.carrying() {
            self.send(Command::Stow(None));
        } else if let Some(t) = self.reached(camera) {
            self.send(Command::Stow(Some(t)));
        }
    }

    /// Puts what the hands hold into the container the crosshair is on, or looks into it.
    pub fn put_in_or_open(&mut self, camera: &Camera) {
        let Some(t) = self.reached(camera) else {
            return;
        };
        self.send(if self.carrying() {
            Command::PutIn(t)
        } else {
            Command::Open(t)
        });
    }
}

/// The inventory window, while `view.inventory` is shown.
pub fn window(ctx: &egui::Context, view: &mut PlanetView, camera: &Camera) {
    let mut shown = view.inventory;
    let inventory = view.session.connection.inventory.clone();
    let reached = view.reached(camera);
    let mut asked = None;
    egui::Window::new("Inventory (I)")
        .open(&mut shown)
        .default_width(330.0)
        .show(ctx, |ui| {
            let load: f64 = (inventory.pockets.iter())
                .flat_map(|p| &p.items)
                .map(|i| i.mass)
                .sum();
            ui.label(format!(
                "In the pockets: {load:.2} kg. E puts away, F puts in or looks into."
            ));
            for (p, pocket) in inventory.pockets.iter().enumerate() {
                let used: f64 = pocket.items.iter().map(|i| i.room).sum();
                ui.separator();
                ui.label(format!(
                    "{}: {:.2} of {:.2} l",
                    pocket.name,
                    used * 1000.0,
                    pocket.volume * 1000.0
                ));
                for (k, item) in pocket.items.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.small_button("take").clicked() {
                            asked = Some(Command::TakeOut {
                                pocket: u8::try_from(p).unwrap_or(u8::MAX),
                                item: u16::try_from(k).unwrap_or(u16::MAX),
                            });
                        }
                        ui.label(&item.text);
                    });
                }
            }
            if let Some((id, store)) = &inventory.open {
                ui.separator();
                let here = reached.filter(|t| t.body == Some(*id));
                ui.label(format!(
                    "In the container ({:.0} l):",
                    store.volume * 1000.0
                ));
                for (k, item) in store.items.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let button =
                            ui.add_enabled(here.is_some(), egui::Button::new("take").small());
                        if let Some(t) = here.filter(|_| button.clicked()) {
                            asked = Some(Command::TakeFrom {
                                container: t,
                                item: u16::try_from(k).unwrap_or(u16::MAX),
                            });
                        }
                        ui.label(&item.text);
                    });
                }
            }
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.label("Creative servers:");
                for (k, name) in kit().iter().enumerate() {
                    let at = target(view, camera).map(|t| t.aimed.point);
                    if ui
                        .add_enabled(at.is_some(), egui::Button::new(name).small())
                        .clicked()
                        && let Some(at) = at
                    {
                        asked = Some(Command::Kit {
                            item: u8::try_from(k).unwrap_or(u8::MAX),
                            at: at + at.normalize() * 0.3,
                        });
                    }
                }
            });
        });
    view.inventory = shown;
    if let Some(command) = asked {
        view.send(command);
    }
}
