//! Fastening parts (G2.3): which fastener of the catalogue the player has chosen, and the
//! command for the parts touching where the crosshair is. Nothing is shown of the joint to
//! come: the player sees the parts as they lie (plan.md, principle 13).

use crate::planet_view::PlanetView;
use creeer_net::message::Command;
use creeer_render::camera::Camera;

/// The catalogue's fasteners by name (`data/fasteners.ron`), and the one chosen.
#[derive(Clone, Debug, Default)]
pub struct Fastening {
    pub names: Vec<String>,
    pub chosen: usize,
}

impl Fastening {
    /// The base packs' fasteners.
    pub fn from_data() -> Self {
        let names = creeer_building::fasteners::Fastener::all(&creeer_data::Data::base())
            .map(|all| all.into_iter().map(|f| f.name).collect())
            .unwrap_or_default();
        Self { names, chosen: 0 }
    }

    /// The chosen fastener's name, if there is any.
    pub fn name(&self) -> Option<&str> {
        self.names.get(self.chosen).map(String::as_str)
    }

    /// The next fastener.
    pub fn next(&mut self) {
        if !self.names.is_empty() {
            self.chosen = (self.chosen + 1) % self.names.len();
        }
    }
}

impl PlanetView {
    /// Fastens the parts touching where the crosshair is on a part, with the chosen fastener.
    pub fn fasten(&mut self, camera: &Camera) {
        let Some(fastener) = self.fastening.name().map(str::to_owned) else {
            return;
        };
        if let Some(target) = self.reached(camera) {
            self.send(Command::Fasten { target, fastener });
        }
    }
}

impl PlanetView {
    /// Takes apart the joint of the built structure nearest where the crosshair is (G2.6).
    pub fn unfasten(&mut self, camera: &Camera) {
        if let Some(target) = self.reached(camera) {
            self.send(Command::Unfasten(target));
        }
    }
}
