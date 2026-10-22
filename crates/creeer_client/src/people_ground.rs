//! The planet's ground and sea as the characters' feet find them (G1.3g): the terrain's surface
//! under a point, by two Newton steps on its density (the height above the ground, whose
//! gradient is exact away from edits: one height lookup each), and the sea's depth at the
//! datum. Bodies (a crate) are not ground here: a character standing on one stands at its own
//! height (`animate::REACH`).

use crate::animate::feet::Ground;
use creeer_math::glam::DVec3;
use creeer_terrain::Terrain;

/// A planet's ground for the animation.
pub struct PlanetGround<'a> {
    pub terrain: &'a Terrain,
}

impl Ground for PlanetGround<'_> {
    fn under(&self, at: DVec3, up: DVec3) -> Option<(DVec3, DVec3)> {
        let mut p = at;
        let mut normal = up;
        for _ in 0..2 {
            let (density, gradient) = self.terrain.density_gradient(p);
            let squared = gradient.length_squared();
            if squared.is_nan() || squared <= 1e-12 {
                return None;
            }
            p -= gradient * (density / squared);
            normal = gradient / squared.sqrt();
        }
        ((p - at).length() < 1.5).then_some((p, normal))
    }

    fn depth(&self, at: DVec3) -> f64 {
        let planet = self.terrain.planet();
        if planet.ocean.is_none() {
            return f64::NEG_INFINITY;
        }
        planet.datum.radius(at) - at.length()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animate::feet::Ground;

    /// A query's cost on the reference planet (G1.3g's budget): a walking character makes
    /// about six a frame.
    #[test]
    fn the_ground_under_a_foot_is_found_quickly_and_on_the_surface() {
        let terrain =
            creeer_server::world::load_terrain(&creeer_data::Data::base(), "planets/home.ron")
                .expect("the reference planet");
        let ground = PlanetGround { terrain: &terrain };
        let up = DVec3::new(0.3, 0.9, 0.2).normalize();
        let surface = up * (terrain.planet().datum.radius(up) + terrain.surface_height(up));
        let queries = 2000u32;
        let start = std::time::Instant::now();
        let mut found = None;
        for k in 0..queries {
            let at = surface + up * 0.5 + DVec3::X * (f64::from(k) * 1e-5);
            found = std::hint::black_box(ground.under(at, up));
        }
        let each = start.elapsed().as_secs_f64() * 1e3 / f64::from(queries);
        println!("a ground query takes {each:.4} ms");
        let (point, normal) = found.expect("ground under the point");
        assert!(
            terrain.density(point).abs() < 1e-3,
            "{}",
            terrain.density(point)
        );
        assert!(normal.dot(up) > 0.5);
        assert!(each * 6.0 < 0.05, "{each} ms a query");
    }
}
