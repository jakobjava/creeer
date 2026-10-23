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
                        swatch(ui, item.colour);
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
                        swatch(ui, item.colour);
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

/// A square of an item's colour: what it looks like (a stone is known by its colour).
fn swatch(ui: &mut egui::Ui, colour: Option<[u8; 3]>) {
    if let Some([r, g, b]) = colour {
        ui.colored_label(egui::Color32::from_rgb(r, g, b), "■");
    }
}

/// Under the crosshair (G1.6): what the hands hold, what the keys would do with it and what the
/// crosshair is on, and what came of the last thing done (or why it was refused) for a few
/// seconds. Nothing a player does is silent, and no key has to be found by trying.
pub fn told(ctx: &egui::Context, view: &PlanetView, aimed: Option<&crate::planet_aim::Target>) {
    let c = &view.session.connection;
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("told"),
    ));
    let mut at = ctx.content_rect().center() + egui::vec2(0.0, 40.0);
    let mut line = |text: &str, colour: egui::Color32| {
        painter.text(
            at,
            egui::Align2::CENTER_TOP,
            text,
            egui::FontId::proportional(14.0),
            colour,
        );
        at.y += 18.0;
    };
    let hands = c.inventory.hands.as_ref();
    let held = hands.map_or_else(|| "Hands: empty".into(), |h| format!("Hands: {}", h.text));
    line(&held, egui::Color32::from_white_alpha(200));
    line(
        &hint(hands, aimed, view.fastening.name()),
        egui::Color32::from_rgb(160, 200, 230),
    );
    let recent = |told: &Option<(String, std::time::Instant)>| {
        told.as_ref()
            .filter(|(_, when)| when.elapsed().as_secs_f64() < 5.0)
            .cloned()
    };
    let (refused, noted) = (recent(&c.refused), recent(&c.noted));
    // The later of the two.
    let last = match (refused, noted) {
        (Some(r), Some(n)) if n.1 > r.1 => Some((n.0, false)),
        (Some(r), _) => Some((r.0, true)),
        (None, Some(n)) => Some((n.0, false)),
        (None, None) => None,
    };
    if let Some((text, refused)) = last {
        let colour = if refused {
            egui::Color32::from_rgb(232, 180, 0)
        } else {
            egui::Color32::from_rgb(200, 230, 170)
        };
        line(&text, colour);
        ctx.request_repaint();
    }
}

/// What the keys would do now, with what the hands hold and what the crosshair is on in reach,
/// with what can be felt (G1.9): a thing's mass, a blow's energy. The keys, not recipes: what
/// makes a good tool is the player's to know.
fn hint(
    hands: Option<&creeer_net::message::ItemView>,
    aimed: Option<&crate::planet_aim::Target>,
    fastener: Option<&str>,
) -> String {
    let aimed = aimed.filter(|t| t.in_reach);
    let on_thing = aimed.is_some_and(|t| t.loose.is_some() || t.aimed.body.is_some());
    let on_tree = aimed.is_some_and(|t| t.tree.is_some());
    let on_ground = aimed.is_some() && !on_thing && !on_tree;
    let on_stone = aimed.is_some_and(|t| {
        ["stone", "flake", "worked stone", "rock"]
            .iter()
            .any(|w| t.what.starts_with(w))
    });
    let lift = match aimed.and_then(|t| t.mass) {
        Some(m) => format!("G: take in the hands ({m:.1} kg)"),
        None => "G: take in the hands".into(),
    };
    let mut keys: Vec<String> = Vec::new();
    match hands {
        None => {
            if on_thing {
                keys.extend([lift, "E: into a pocket".into()]);
                // (Two parts touching there are fastened: the hands must be free, G2.3.)
                if let Some(f) = fastener {
                    keys.push(format!(
                        "J: fasten with {f} (K: another) · U: take a joint apart"
                    ));
                }
            }
            if on_ground {
                keys.push("click: dig with the hands".into());
            }
        }
        Some(h) => {
            let blow = format!("{}, {:.1} kg, {:.0} J", h.word, h.mass, h.swing);
            if on_thing || on_tree {
                keys.push(format!("click: strike ({blow})"));
            } else if on_ground {
                keys.push(format!("click: dig ({blow})"));
            }
            if h.word == "stick" {
                keys.push("R: strip bark".into());
                if on_stone {
                    keys.push("B: bind it on as a head".into());
                }
            }
            keys.extend(["G: put down", "T: throw", "E: into a pocket"].map(String::from));
        }
    }
    keys.push("I: what you carry".into());
    keys.join("  ·  ")
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
