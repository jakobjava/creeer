//! Where avatars first appear: flat land at a temperate latitude (G1.3i).

use creeer_math::det;
use creeer_math::glam::DVec3;
use creeer_terrain::Terrain;

/// A point on the ground of `terrain`: on land between 300 and 1500 m, at a temperate latitude
/// (30-45°, the planet turning about +Y), where the noon sun stands 45-60° high at the equinox;
/// the first such of a spread of directions, so always the same one. (Until G1.3i the first
/// land of the spread, by the pole, where the sun never rose above 4°.)
pub fn spawn_point(terrain: &Terrain) -> DVec3 {
    let (low, high) = (det::sin(30f64.to_radians()), det::sin(45f64.to_radians()));
    let up = creeer_terrain::elevation::fibonacci_sphere(5000)
        .filter(|d| (low..=high).contains(&d.y.abs()))
        .filter(|&d| (300.0..1500.0).contains(&terrain.surface_height(d)))
        .find(|&d| flat(terrain, d))
        .unwrap_or(DVec3::Y);
    ground_under(
        terrain,
        up * (terrain.planet().radius + terrain.surface_height(up) + 50.0),
    )
}

/// Whether the ground about `up` is flat enough to build on: rising or falling at most 1.5° in
/// every direction out to 5 m, 15 m and 40 m.
fn flat(terrain: &Terrain, up: DVec3) -> bool {
    let radius = terrain.planet().radius;
    let (east, north) = creeer_terrain::geology::deposits::tangent_frame(up);
    let here = terrain.surface_height(up);
    let limit = det::tan(1.5f64.to_radians());
    (0..8).all(|k| {
        let (s, c) = det::sin_cos(f64::from(k) * std::f64::consts::FRAC_PI_4);
        [5.0, 15.0, 40.0].iter().all(|&d| {
            let p = (up * radius + (east * c + north * s) * d).normalize();
            (terrain.surface_height(p) - here).abs() <= limit * d
        })
    })
}

/// The point of the ground of `terrain` under (or over) `p`, along the vertical.
pub(super) fn ground_under(terrain: &Terrain, mut p: DVec3) -> DVec3 {
    let up = p.normalize();
    // The density is the height above the ground: down by it until on it.
    for _ in 0..30 {
        let d = terrain.density(p);
        p -= up * d;
        if d.abs() < 1e-4 {
            break;
        }
    }
    p
}
