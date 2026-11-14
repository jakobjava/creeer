//! What the player carries (G1.6a, ADR-0028): E puts away what the hands hold or the item the
//! crosshair is on, F puts what the hands hold into the container the crosshair is on (or looks
//! into it), I shows the pockets and the container looked into, each item with what can be seen
//! of it, to take out into the hands. At a glance, the same at the bottom right
//! (`hud_hands.rs`).

use crate::bindings::{Action, Bindings};
use crate::planet_aim::target;
use crate::planet_view::PlanetView;
use creeer_net::message::{Command, Target};
use creeer_render::Camera;
use creeer_ui::egui::{self, RichText};
use creeer_ui::theme::{HIGHLIGHT, TEXT_DIM};

impl PlanetView {
    /// What the crosshair is on as a target, if it is a body within reach.
    pub(crate) fn reached(&self, camera: &Camera) -> Option<Target> {
        let t = target(self, camera).filter(|t| t.in_reach && t.aimed.body.is_some())?;
        Some(Target {
            body: t.aimed.body,
            at: t.aimed.point,
            seen: t.aimed.seen,
        })
    }

    /// Puts away what the hands hold, or the item the crosshair is on.
    pub fn stow(&mut self, camera: &Camera) {
        let loose = target(self, camera)
            .filter(|t| t.in_reach)
            .and_then(|t| t.loose);
        if self.carrying() {
            self.send(Command::Stow(None));
        } else if let Some(loose) = loose {
            self.send(Command::Pick {
                loose,
                pocket: true,
            });
        } else if let Some(t) = self.reached(camera) {
            self.send(Command::Stow(Some(t)));
        }
    }

    /// Binds the head the crosshair is on to the haft in the hands (G1.6d).
    pub fn bind(&mut self, camera: &Camera) {
        let loose = target(self, camera)
            .filter(|t| t.in_reach)
            .and_then(|t| t.loose);
        if let Some(id) = loose {
            self.send(Command::BindLying(id));
        } else if let Some(t) = self.reached(camera) {
            self.send(Command::Bind(t));
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

/// The inventory window, while `view.inventory` is shown: what the hands hold, each pocket
/// with how full it is and what is in it (each to take out into the hands), and the container
/// looked into (G2.11 A3: laid out for reading).
pub fn window(ctx: &egui::Context, view: &mut PlanetView, camera: &Camera, keys: &Bindings) {
    if !view.inventory {
        return;
    }
    let mut shown = true;
    let inventory = view.session.connection.inventory.clone();
    let reached = view.reached(camera);
    let mut asked = None;
    egui::Window::new("What you carry")
        .open(&mut shown)
        .resizable(false)
        .collapsible(false)
        .default_pos([24.0, 120.0])
        .show(ctx, |ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            let hint = format!(
                "{}: into a pocket   ·   {}: into a container, or look in",
                keys.name(Action::Pocket),
                keys.name(Action::PutIn)
            );
            ui.label(RichText::new(hint).color(TEXT_DIM));
            ui.add_space(4.0);
            ui.label(RichText::new("In your hands").strong().color(HIGHLIGHT));
            match &inventory.hands {
                Some(item) => item_row(ui, item, None),
                None => {
                    ui.label(RichText::new("nothing").color(TEXT_DIM));
                }
            }
            for (p, pocket) in inventory.pockets.iter().enumerate() {
                ui.add_space(6.0);
                store_header(ui, &crate::hud_hands::pocket_name(&pocket.name), pocket);
                for (k, item) in pocket.items.iter().enumerate() {
                    let take = Command::TakeOut {
                        pocket: u8::try_from(p).unwrap_or(u8::MAX),
                        item: u16::try_from(k).unwrap_or(u16::MAX),
                    };
                    item_row(ui, item, Some((&mut asked, Some(take))));
                }
            }
            if let Some((id, store)) = &inventory.open {
                ui.add_space(6.0);
                store_header(ui, "The container looked into", store);
                let here = reached.filter(|t| t.body == Some(*id));
                for (k, item) in store.items.iter().enumerate() {
                    let take = here.map(|t| Command::TakeFrom {
                        container: t,
                        item: u16::try_from(k).unwrap_or(u16::MAX),
                    });
                    item_row(ui, item, Some((&mut asked, take)));
                }
            }
        });
    view.inventory = shown;
    if let Some(command) = asked {
        view.send(command);
    }
}

/// A store's heading: its name, how full it is as a bar, and its litres.
fn store_header(ui: &mut egui::Ui, name: &str, store: &creeer_net::message::StoreView) {
    let share = crate::hud_hands::fill(store);
    ui.horizontal(|ui| {
        ui.label(RichText::new(name).strong().color(HIGHLIGHT));
        let (rect, _) = ui.allocate_exact_size(egui::vec2(60.0, 10.0), egui::Sense::hover());
        creeer_ui::paint::bar(ui.painter(), rect, share, TEXT_DIM);
        // (An empty sum is -0: plus 0 it is 0, not "-0.00".)
        let used = store.items.iter().map(|i| i.room).sum::<f64>() + 0.0;
        let litres = format!("{:.2} of {:.2} l", used * 1e3, store.volume * 1e3);
        ui.label(RichText::new(litres).color(TEXT_DIM));
    });
    if store.items.is_empty() {
        ui.label(RichText::new("empty").color(TEXT_DIM));
    }
}

/// A take button's: where the command asked goes, and the command (none: not within reach).
type Take<'a> = Option<(&'a mut Option<Command>, Option<Command>)>;

/// An item: a square of its colour (a stone is known by its colour), its word, what can be seen
/// of it, and a button to take it into the hands (greyed out: not within reach).
fn item_row(ui: &mut egui::Ui, item: &creeer_net::message::ItemView, take: Take<'_>) {
    ui.horizontal(|ui| {
        if let Some([r, g, b]) = item.colour {
            ui.label(RichText::new("■").color(egui::Color32::from_rgb(r, g, b)));
        }
        ui.label(RichText::new(&item.word).strong());
        ui.label(RichText::new(item.text.replace('_', " ")).color(TEXT_DIM));
        if let Some((asked, command)) = take {
            let button = ui.add_enabled(command.is_some(), egui::Button::new("Take"));
            if button.clicked() {
                *asked = command;
            }
        }
    });
}

/// The vitals (G1.9), at the bottom left: health, what each of the body's parts has taken, and
/// the jetpack's gas while there is one (survival players have none).
pub fn vitals(ctx: &egui::Context, c: &creeer_server::Connection) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("vitals"),
    ));
    let corner = ctx.content_rect().left_bottom() + egui::vec2(16.0, -16.0);
    painter.text(
        corner,
        egui::Align2::LEFT_BOTTOM,
        vitals_text(c.health, c.hurt, c.fallen, c.gas),
        egui::FontId::proportional(14.0),
        egui::Color32::from_white_alpha(210),
    );
}

/// The vitals as a line: health, each hurt part's loss (a part unhurt is left out), down, gas.
fn vitals_text(health: f64, hurt: [f64; 4], fallen: bool, gas: f64) -> String {
    let mut parts = vec![format!("Health {:.0} %", health * 100.0)];
    for (name, h) in creeer_sim::character::ragdoll::PARTS.iter().zip(hurt) {
        if h >= 0.005 {
            parts.push(format!("{name} −{:.0} %", h * 100.0));
        }
    }
    if fallen {
        parts.push("down".into());
    }
    if gas > 0.0 {
        parts.push(format!("jetpack gas {gas:.2} kg"));
    }
    parts.join("  ·  ")
}

#[cfg(test)]
mod vitals_tests {
    #[test]
    fn the_vitals_name_only_hurt_parts_and_gas_only_with_a_jetpack() {
        let text = super::vitals_text(0.7, [0.0, 0.25, 0.05, 0.0], false, 0.0);
        assert_eq!(text, "Health 70 %  ·  torso −25 %  ·  arms −5 %");
        let flying = super::vitals_text(1.0, [0.0; 4], true, 1.5);
        assert_eq!(flying, "Health 100 %  ·  down  ·  jetpack gas 1.50 kg");
    }
}
