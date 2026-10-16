//! A planet in the physics world: its terrain as ground, its gravity in its rotating frame.
//!
//! The physics world runs in the planet's frame (centre at the origin, rotation about +Y):
//! the terrain stands still in it, and gravity includes the centrifugal and Coriolis
//! accelerations of that frame (`creeer_physics::gravity`). The terrain's sea level is the
//! equipotential of the same field (`creeer_terrain::datum`), so level ground is level for
//! the physics too.

use creeer_math::glam::DVec3;
use creeer_physics::gravity::PlanetGravity;
use creeer_physics::world::ground::Ground;
use creeer_terrain::{Planet, Terrain};
use std::sync::Arc;

/// The planet's gravity as seen from its rotating frame.
pub fn planet_gravity(planet: &Planet) -> PlanetGravity {
    PlanetGravity {
        mu: planet.mu,
        spin: DVec3::Y * planet.angular_speed(),
    }
}

/// A planet's terrain as the physics world's ground (edits included).
#[derive(Clone, Debug)]
pub struct TerrainGround(pub Arc<Terrain>);

impl Ground for TerrainGround {
    fn density_gradient(&self, p: DVec3) -> (f64, DVec3) {
        self.0.density_gradient(p)
    }
}
