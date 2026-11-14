//! The menu's part in the app (G2.11 A3, ADR-0038): Esc opening and closing it (and what is
//! open on top first), what its buttons ask, a key being rebound, the settings applied each
//! frame and saved, and the cursor: captured while walking on the planet with nothing open, so
//! the mouse looks about; free otherwise (flying, the tech demo: the right mouse looks).

use super::*;
use crate::bindings::{Action, Input, entry, input_name};
use crate::menu::{Asked, Page};

impl Running {
    /// Esc: lets go of what is open on top (a key being rebound, a page of the menu, the kit,
    /// the inventory), else opens or closes the menu.
    pub(super) fn escape(&mut self) {
        if self.screen.rebinding.take().is_some() {
            return;
        }
        match self.screen.menu {
            Some(Page::Main) => self.close_menu(),
            Some(_) => self.screen.menu = Some(Page::Main),
            None if self.screen.kit => self.screen.kit = false,
            None if self.planet.as_ref().is_some_and(|p| p.inventory) => {
                if let Some(p) = &mut self.planet {
                    p.inventory = false;
                }
            }
            None => self.open_menu(),
        }
    }

    pub(super) fn open_menu(&mut self) {
        self.screen.menu = Some(Page::Main);
        self.release_keys();
    }

    fn close_menu(&mut self) {
        self.screen.menu = None;
        self.screen.rebinding = None;
        if self.screen.dirty {
            self.save_settings();
        }
    }

    /// Every held key let go (a menu opened, the window left), so none stays down.
    pub(super) fn release_keys(&mut self) {
        self.fly.release_all();
        if let Some(h) = self.holding() {
            h.key(Action::Turn, false);
            h.key(Action::Fine, false);
        }
    }

    /// The window lost the keyboard: keys let go, and on the planet the menu opened (the game
    /// is not played blind meanwhile).
    pub(super) fn focus_lost(&mut self) {
        self.release_keys();
        if self.captured && self.screen.menu.is_none() {
            self.open_menu();
        }
        self.captured = false;
        self.set_looking(false);
    }

    /// Writes the settings file (if the environment names a place for it).
    pub(super) fn save_settings(&mut self) {
        self.screen.dirty = false;
        if let Some(path) = &self.game_file
            && let Err(e) = self.game.save(path)
        {
            eprintln!("settings: {e}");
        }
    }

    /// `input` pressed while `action` waits for a key: it is the action's now (Esc: not).
    pub(super) fn rebind(&mut self, action: Action, input: Input) {
        self.screen.rebinding = None;
        if self.game.bindings.of(Action::Menu).contains(&input) {
            return;
        }
        let name = input_name(input);
        let told = match self.game.bindings.bind(action, input) {
            Some(lost) if lost.is_empty() => format!("{name}: {}", entry(action).name),
            Some(lost) => {
                let names: Vec<&str> = lost.iter().map(|a| entry(*a).name).collect();
                let unbound = names.join("; ");
                format!("{name}: {} (no longer: {unbound})", entry(action).name)
            }
            None => format!("{name} cannot be used for that"),
        };
        self.screen.tell(told);
        self.screen.dirty = true;
    }

    /// Does what the menu asked.
    pub(super) fn menu_asked(&mut self, asked: Asked) {
        if asked.resume {
            self.close_menu();
        }
        if asked.save {
            let told = self.planet.as_ref().map(crate::planet_ui::save_world);
            match told {
                Some(Ok(where_)) => self.screen.tell(where_),
                Some(Err(why)) => self.screen.tell(format!("Not saved: {why}")),
                None => {}
            }
        }
        if asked.leave {
            self.close_menu();
            self.leave_planet();
        }
        if asked.quit {
            if self.screen.dirty {
                self.save_settings();
            }
            self.quit = true;
        }
        // The controls seen once: the hint saying how to find them is not shown again.
        if self.screen.menu == Some(Page::Controls) && !self.game.controls_seen {
            self.game.controls_seen = true;
            self.screen.dirty = true;
        }
    }

    /// Each frame: the interface's size and the field of view as set, and the cursor captured
    /// or freed.
    pub(super) fn apply_settings(&mut self) {
        if (self.egui_ctx.zoom_factor() - self.game.ui_scale).abs() > 1e-3 {
            self.egui_ctx.set_zoom_factor(self.game.ui_scale);
        }
        self.camera.fov_y = self.game.field_of_view.to_radians();
        // The sound's volume as set (ADR-0037's `Sounds::set_volumes`).
        if let Some(planet) = self.planet.as_mut() {
            let mut volumes = planet.sound.volumes();
            if (volumes.master - self.game.volume).abs() > 1e-4 {
                volumes.master = self.game.volume;
                planet.sound.set_volumes(volumes);
            }
        }
        let walking = (self.planet.as_ref()).is_some_and(|p| p.walking && !p.inventory);
        let capture = walking && self.screen.menu.is_none() && !self.screen.kit;
        if capture != self.captured {
            self.captured = capture;
            self.set_looking(capture);
        }
    }
}
