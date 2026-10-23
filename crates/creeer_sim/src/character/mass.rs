//! A character's mass (G1.2c, G1.6a): its dry mass, its jetpack's gas and what its pockets
//! hold, all on its body.

use super::{DRY, MASS};
use creeer_physics::world::Body;

/// The gas left in a character's jetpack (kg), its body carrying `load` (kg) besides.
pub fn gas(body: &Body, load: f64) -> f64 {
    (body.structure.masses()[0] - DRY - load).max(0.0)
}

/// Character `body`'s load changed from `from` to `to` (kg): its mass with it, its gas kept.
pub fn set_load(body: &mut Body, from: f64, to: f64) {
    let mass = body.structure.masses()[0] - from + to;
    body.solver.set_mass(&mut body.structure, 0, mass);
}

/// Fills a character's jetpack (on creative servers only: in survival nothing refills it, G1.6b).
pub fn refill(body: &mut Body, load: f64) {
    body.solver.set_mass(&mut body.structure, 0, MASS + load);
}
