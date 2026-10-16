//! Terrain for every body with a surface, loaded when first needed.

use creeer_orbit::{Body, BodyKind};
use creeer_terrain::{DataError, Overrides, Planet, Terrain};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Which body a terrain belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BodyRef {
    /// A major body: index into the ephemeris.
    Major(usize),
    /// A minor body: index into the universe's minors.
    Minor(usize),
}

/// The terrain of a body: its own file for the hand-made home planet, a template with the
/// body's numbers for generated ones (`Planet::load_with`).
pub fn load(
    body: &Body,
    data: &std::path::Path,
    home_file: &str,
) -> Option<Result<Terrain, DataError>> {
    let file = body.surface.as_deref()?;
    if !body.kind.has_surface() {
        return None;
    }
    let path = data.join(file);
    let planet = if file == home_file {
        Planet::load(&path)
    } else {
        let overrides = Overrides {
            name: body.name.clone(),
            seed: body.seed,
            radius: body.radius,
            mu: body.mu,
            sidereal_day: std::f64::consts::TAU / body.spin.rate,
            axial_tilt: body.spin.axis().angle_between(creeer_math::glam::DVec3::Y),
        };
        Planet::load_with(&path, Some(&overrides))
    };
    Some(planet.map(Terrain::new))
}

/// Loaded terrains.
#[derive(Debug, Default)]
pub struct Terrains {
    data: PathBuf,
    home_file: String,
    loaded: HashMap<BodyRef, Arc<Terrain>>,
}

impl Terrains {
    /// Terrains read from the data directory `data`; `home_file` is the home planet's file.
    pub fn new(data: PathBuf, home_file: String) -> Self {
        Self {
            data,
            home_file,
            loaded: HashMap::new(),
        }
    }

    /// The terrain of `body` (`which`), loading it on first use; `None` for bodies without a
    /// surface or whose data fails to load (the error is printed once).
    pub fn get(&mut self, which: BodyRef, body: &Body) -> Option<Arc<Terrain>> {
        if let Some(t) = self.loaded.get(&which) {
            return Some(Arc::clone(t));
        }
        match load(body, &self.data, &self.home_file)? {
            Ok(terrain) => {
                let terrain = Arc::new(terrain);
                self.loaded.insert(which, Arc::clone(&terrain));
                Some(terrain)
            }
            Err(e) => {
                eprintln!("terrain of {} ({:?}): {e}", body.name, body.kind);
                None
            }
        }
    }

    /// An already loaded terrain.
    pub fn loaded(&self, which: BodyRef) -> Option<&Arc<Terrain>> {
        self.loaded.get(&which)
    }

    /// Whether `kind` ever has terrain.
    pub fn has_surface(kind: BodyKind) -> bool {
        kind.has_surface()
    }
}
