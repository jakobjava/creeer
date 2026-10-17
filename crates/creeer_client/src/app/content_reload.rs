//! Hot-reload of the data packs (F7.1, ADR-0018): the packs are polled twice a second; a
//! valid change replaces the data and the open sandbox starts again on it (the planet
//! sandbox's singleplayer world, the solar system, the physics scene, the lab); a broken one
//! keeps the old data and shows why. A dedicated server's game keeps the server's data.

use super::*;
use crate::content::{self, Reload};

/// What the last reload did, for the Sandbox window.
#[derive(Debug, Default)]
pub(crate) struct ContentStatus {
    pub notice: String,
    pub broken: bool,
}

impl Running {
    /// Looks for changed packs and applies them.
    pub(super) fn watch_content(&mut self) {
        if !self.watcher.poll() {
            return;
        }
        match content::reload() {
            Reload::Same => {}
            Reload::Broken(why) => {
                eprintln!("data not reloaded:\n{why}");
                self.content.notice = format!("not reloaded: {why}");
                self.content.broken = true;
            }
            Reload::Changed(hash) => {
                println!("data reloaded (content {hash:016x})");
                self.content.notice = format!("reloaded (content {hash:016x})");
                self.content.broken = false;
                self.apply_content();
            }
        }
    }

    /// Starts the open sandboxes again on the new data.
    fn apply_content(&mut self) {
        self.lab.reload();
        self.physics.restart();
        if self.planet.as_ref().is_some_and(|p| p.session.hosted()) {
            let camera = self.camera;
            self.enter_planet();
            self.camera = camera;
        }
        if self.system.is_some() {
            self.enter_system();
        }
    }

    /// The data's line in the Sandbox window.
    pub(super) fn content_line(&self, ui: &mut egui::Ui) {
        let data = content::data();
        let packs: Vec<&str> = data.packs().iter().map(creeer_data::Pack::name).collect();
        ui.label(format!(
            "Data: {} ({:016x})",
            packs.join(" + "),
            data.hash()
        ));
        if !self.content.notice.is_empty() {
            let colour = if self.content.broken {
                egui::Color32::LIGHT_RED
            } else {
                egui::Color32::LIGHT_GREEN
            };
            ui.colored_label(colour, &self.content.notice);
        }
    }
}
