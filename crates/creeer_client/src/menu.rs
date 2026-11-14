//! The game's menu (Esc; G2.11 A3, ADR-0038): resume, the controls (the table's, rebindable:
//! `menu_controls.rs`), the settings (mouse, view, interface size, volume, the character's
//! look), saving the world, leaving the planet and quitting. The world does not stop meanwhile:
//! it is a server's, which other players may share.

use crate::bindings::Action;
use crate::planet_view::PlanetView;
use crate::settings::{FIELD_OF_VIEW, GameSettings, SENSITIVITY, UI_SCALE};
use creeer_ui::egui::{self, Align2, Color32, RichText, vec2};
use creeer_ui::theme::{HEADING, TEXT_DIM};
use std::time::Instant;

/// A page of the menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Main,
    Controls,
    Settings,
}

/// What the screen shows besides the world: the menu and its page, the debug view (F3), the
/// creative kit (C), a key being rebound, and the menu's own last answer.
#[derive(Debug, Default)]
pub(crate) struct Screen {
    pub menu: Option<Page>,
    pub debug: bool,
    pub kit: bool,
    pub rebinding: Option<Action>,
    /// Whether the world is a creative server's this process hosts (it shows what is built:
    /// `Host::inspection`): the kit and the stress overlay are then told at the hands' panel.
    pub creative: bool,
    /// What failed, told at the top (`stress_view::hud`): the server's note of it is not told
    /// again above the crosshair.
    pub failed: Vec<String>,
    /// Settings changed and not yet saved (saved as the menu closes).
    pub dirty: bool,
    pub note: Option<(String, Instant)>,
}

impl Screen {
    /// Tells `text` in the menu for a few seconds.
    pub fn tell(&mut self, text: impl Into<String>) {
        self.note = Some((text.into(), Instant::now()));
    }

    /// Esc: lets go of what is open on top (a key being rebound, a page of the menu, the kit,
    /// the inventory `inventory` says is shown), else opens or closes the menu. What it did.
    pub fn escape(&mut self, inventory: Option<&mut bool>) -> Escaped {
        if self.rebinding.take().is_some() {
            return Escaped::Other;
        }
        match self.menu {
            Some(Page::Main) => {
                self.menu = None;
                self.rebinding = None;
                return Escaped::Closed;
            }
            Some(_) => self.menu = Some(Page::Main),
            None if self.kit => self.kit = false,
            None => match inventory {
                Some(shown) if *shown => *shown = false,
                _ => {
                    self.menu = Some(Page::Main);
                    return Escaped::Opened;
                }
            },
        }
        Escaped::Other
    }
}

/// What Esc did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Escaped {
    /// The menu opened (every held key is let go) or closed (the settings saved if changed).
    Opened,
    Closed,
    /// Something on top closed, or a page went back.
    Other,
}

/// What the menu asks of the app.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Asked {
    pub resume: bool,
    pub save: bool,
    pub leave: bool,
    pub quit: bool,
}

/// A menu button's size (points).
const BUTTON: egui::Vec2 = vec2(280.0, 34.0);

/// Draws the menu, if open: a dimmed world behind it, the page in the middle.
pub(crate) fn draw(
    ctx: &egui::Context,
    screen: &mut Screen,
    game: &mut GameSettings,
    planet: Option<&mut PlanetView>,
) -> Asked {
    let mut asked = Asked::default();
    let Some(page) = screen.menu else {
        return asked;
    };
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Background,
        egui::Id::new("menu dim"),
    ));
    painter.rect_filled(ctx.content_rect(), 0.0, Color32::from_black_alpha(140));
    let hosted = planet.as_ref().is_some_and(|p| p.session.hosted());
    let on_planet = planet.is_some();
    egui::Window::new("menu")
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(title(page)).size(HEADING).strong());
            });
            ui.separator();
            match page {
                Page::Main => main(ui, screen, &mut asked, (hosted, on_planet)),
                Page::Controls => crate::menu_controls::page(ui, screen, game),
                Page::Settings => settings(ui, screen, game, planet),
            }
            if page != Page::Main {
                ui.separator();
                if ui.add(egui::Button::new("Back").min_size(BUTTON)).clicked() {
                    screen.menu = Some(Page::Main);
                }
            }
            if let Some((note, when)) = &screen.note
                && when.elapsed().as_secs_f32() < 5.0
            {
                ui.label(RichText::new(note).color(TEXT_DIM));
            }
        });
    asked
}

fn title(page: Page) -> &'static str {
    match page {
        Page::Main => "Creëer",
        Page::Controls => "Controls",
        Page::Settings => "Settings",
    }
}

/// The main page's buttons, one under the other.
fn main(ui: &mut egui::Ui, screen: &mut Screen, asked: &mut Asked, (hosted, planet): (bool, bool)) {
    let button = |ui: &mut egui::Ui, text: &str| {
        ui.add(egui::Button::new(RichText::new(text).size(17.0)).min_size(BUTTON))
            .clicked()
    };
    ui.vertical_centered(|ui| {
        asked.resume = button(ui, "Resume");
        if button(ui, "Controls") {
            screen.menu = Some(Page::Controls);
        }
        if button(ui, "Settings") {
            screen.menu = Some(Page::Settings);
        }
        if hosted {
            asked.save = button(ui, "Save the world");
        }
        if planet {
            asked.leave = button(ui, "Leave the planet");
        }
        asked.quit = button(ui, "Quit the game");
    });
}

/// The settings page: each change made at once, saved as the menu closes.
fn settings(
    ui: &mut egui::Ui,
    screen: &mut Screen,
    game: &mut GameSettings,
    planet: Option<&mut PlanetView>,
) {
    let before = game.clone();
    egui::Grid::new("settings")
        .num_columns(2)
        .spacing([16.0, 10.0])
        .show(ui, |ui| {
            ui.label("Mouse sensitivity");
            let (low, high) = SENSITIVITY;
            ui.add(
                egui::Slider::new(&mut game.mouse_sensitivity, low..=high)
                    .logarithmic(true)
                    .suffix("×"),
            );
            ui.end_row();
            ui.label("Invert the mouse's up and down");
            ui.checkbox(&mut game.invert_y, "");
            ui.end_row();
            ui.label("Field of view (top to bottom)");
            let (low, high) = FIELD_OF_VIEW;
            ui.add(egui::Slider::new(&mut game.field_of_view, low..=high).suffix("°"));
            ui.end_row();
            ui.label("Interface size");
            let (low, high) = UI_SCALE;
            // Applied when let go of, so the slider does not run from under the mouse.
            let mut scale = game.ui_scale;
            let r = ui.add(egui::Slider::new(&mut scale, low..=high).suffix("×"));
            if r.drag_stopped() || (r.changed() && !r.dragged()) {
                game.ui_scale = scale;
            }
            ui.end_row();
            ui.label("Volume");
            let mut percent = game.volume * 100.0;
            if ui
                .add(egui::Slider::new(&mut percent, 0.0..=100.0).suffix(" %"))
                .changed()
            {
                game.volume = percent / 100.0;
            }
            ui.end_row();
            ui.label("What the crosshair is on");
            ui.horizontal(|ui| {
                ui.radio_value(&mut game.prompts_beside, false, "at the bottom");
                ui.radio_value(&mut game.prompts_beside, true, "beside it");
            });
            ui.end_row();
        });
    if let Some(view) = planet {
        ui.separator();
        ui.label("Your look");
        crate::planet_ui::look(ui, view);
    }
    if *game != before {
        screen.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each page draws without a window or a planet, asks nothing unasked, and leaves the
    /// settings as they were.
    #[test]
    fn every_page_draws_and_asks_nothing_by_itself() {
        let ctx = egui::Context::default();
        creeer_ui::theme::apply(&ctx);
        let mut game = GameSettings::default();
        for page in [
            None,
            Some(Page::Main),
            Some(Page::Controls),
            Some(Page::Settings),
        ] {
            let mut screen = Screen {
                menu: page,
                rebinding: Some(Action::Fasten),
                ..Screen::default()
            };
            screen.tell("Saved");
            for _ in 0..2 {
                let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                    let asked = draw(ui.ctx(), &mut screen, &mut game, None);
                    assert_eq!(asked, Asked::default());
                });
                output.textures_delta.clear();
            }
            assert_eq!(screen.menu, page, "no page changed by itself");
            assert!(!screen.dirty);
        }
        assert_eq!(game, GameSettings::default());
    }
}
