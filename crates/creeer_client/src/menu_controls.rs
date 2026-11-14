//! The menu's controls page (G2.11 A3, ADR-0038): every action of the controls' table
//! (`bindings.rs`) under its heading, with its key; a click on a key, then the key wanted,
//! moves the action to it (`app/input.rs` takes that key). Built from the table alone.

use crate::bindings::{Action, TABLE, entry};
use crate::menu::Screen;
use crate::settings::GameSettings;
use creeer_ui::egui::{self, RichText, vec2};
use creeer_ui::theme::{BAD, HIGHLIGHT, TEXT_DIM};

/// Draws the page.
pub(crate) fn page(ui: &mut egui::Ui, screen: &mut Screen, game: &mut GameSettings) {
    ui.label(
        RichText::new("Click a key, then press the key or button you want for it.").color(TEXT_DIM),
    );
    let height = ui.ctx().content_rect().height() * 0.6;
    egui::ScrollArea::vertical()
        .max_height(height)
        .show(ui, |ui| {
            let mut group = "";
            for e in TABLE {
                if e.group != group {
                    group = e.group;
                    ui.add_space(6.0);
                    ui.label(RichText::new(group).strong().color(HIGHLIGHT));
                }
                ui.horizontal(|ui| {
                    key(ui, screen, game, e.action);
                    ui.label(e.name);
                });
            }
        });
    ui.add_space(4.0);
    if ui.button("Every key back to its default").clicked() {
        game.bindings = crate::bindings::Bindings::default();
        screen.rebinding = None;
        screen.dirty = true;
        screen.tell("The default keys are back.");
    }
}

/// An action's key as a button: clicked, the next key pressed is taken for it.
fn key(ui: &mut egui::Ui, screen: &mut Screen, game: &GameSettings, action: Action) {
    let e = entry(action);
    let waiting = screen.rebinding == Some(action);
    let clashes = game.bindings.clashes(action);
    let text = if waiting {
        RichText::new("press a key…").color(HIGHLIGHT)
    } else {
        let names: Vec<String> = (game.bindings.of(action).iter())
            .map(|i| crate::bindings::input_name(*i))
            .collect();
        let shown = if names.is_empty() {
            "unbound".to_owned()
        } else {
            names.join(" or ")
        };
        let text = RichText::new(shown);
        if clashes.is_empty() && !names.is_empty() {
            text
        } else {
            text.color(BAD)
        }
    };
    let button = egui::Button::new(text).min_size(vec2(130.0, 24.0));
    let response = ui.add_enabled(!e.fixed, button);
    let response = if clashes.is_empty() {
        response
    } else {
        let also: Vec<&str> = clashes.iter().map(|a| entry(*a).name).collect();
        response.on_hover_text(format!("Also: {}", also.join("; ")))
    };
    if response.clicked() {
        screen.rebinding = Some(action);
        // So the key pressed next is not a click of this button again.
        response.surrender_focus();
    }
}
