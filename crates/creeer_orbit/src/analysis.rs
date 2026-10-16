//! Orbital analysis: which body something orbits, sphere-of-influence and Hill radii, Roche
//! limits, Lagrange points, and the classic transfer formulas.
//!
//! None of this steers the physics (everything pulls on everything, ADR-0007); it is what
//! the map view, the generator and the player's calculations use.

use creeer_math::det;
use creeer_math::glam::DVec3;

/// Laplace's sphere-of-influence radius of a body of `mu` orbiting `mu_primary` at
/// distance `a` (m): a·(μ/μₚ)^{2/5}.
pub fn sphere_of_influence(a: f64, mu: f64, mu_primary: f64) -> f64 {
    a * det::pow(mu / mu_primary, 0.4)
}

/// Hill radius (m): a·(1 − e)·∛(μ/3μₚ). Moons are long-term stable within about half of it
/// (prograde) — the generator's limit.
pub fn hill_radius(a: f64, e: f64, mu: f64, mu_primary: f64) -> f64 {
    a * (1.0 - e) * det::cbrt(mu / (3.0 * mu_primary))
}

/// Rigid-body Roche limit (m): inside it a moon held together by its own gravity alone is
/// torn apart by its primary's tides: d = R·∛(2ρₚ/ρₘ) (fluid bodies: 2.44·R·∛(ρₚ/ρₘ)).
pub fn roche_limit(primary_radius: f64, primary_density: f64, moon_density: f64) -> f64 {
    primary_radius * det::cbrt(2.0 * primary_density / moon_density)
}

/// Speed needed to escape from distance `r` (m/s).
pub fn escape_speed(mu: f64, r: f64) -> f64 {
    (2.0 * mu / r).sqrt()
}

/// Speed of a circular orbit of radius `r` (m/s).
pub fn circular_speed(mu: f64, r: f64) -> f64 {
    (mu / r).sqrt()
}

/// Speed at distance `r` on an orbit of semi-major axis `a` (vis-viva, m/s).
pub fn vis_viva(mu: f64, r: f64, a: f64) -> f64 {
    (mu * (2.0 / r - 1.0 / a)).sqrt()
}

/// The two burns of a Hohmann transfer between circular orbits of radii `r1` and `r2`
/// (m/s), and the transfer time (s).
pub fn hohmann(mu: f64, r1: f64, r2: f64) -> (f64, f64, f64) {
    let a = 0.5 * (r1 + r2);
    let dv1 = (vis_viva(mu, r1, a) - circular_speed(mu, r1)).abs();
    let dv2 = (circular_speed(mu, r2) - vis_viva(mu, r2, a)).abs();
    (dv1, dv2, std::f64::consts::PI * (a * a * a / mu).sqrt())
}

/// For every body, the body it orbits (`None` for the most massive), chosen as the
/// heaviest-first hierarchy of spheres of influence: a body belongs to the smallest sphere
/// of influence of a heavier body that contains it.
pub fn primaries(mu: &[f64], x: &[DVec3]) -> Vec<Option<usize>> {
    let n = mu.len();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| mu[b].total_cmp(&mu[a]).then(a.cmp(&b)));
    let mut parent = vec![None; n];
    let mut soi = vec![f64::INFINITY; n];
    for (rank, &i) in order.iter().enumerate() {
        if rank == 0 {
            continue;
        }
        let mut best: Option<usize> = None;
        for &j in &order[..rank] {
            let inside = (x[i] - x[j]).length() < soi[j];
            if inside && best.is_none_or(|b| soi[j] < soi[b]) {
                best = Some(j);
            }
        }
        parent[i] = best;
        if let Some(p) = best {
            soi[i] = sphere_of_influence((x[i] - x[p]).length(), mu[i], mu[p]);
        }
    }
    parent
}

/// The body an object at `x` orbits, given the bodies' hierarchy from [`primaries`]: the
/// smallest sphere of influence that contains it.
pub fn primary_of(mu: &[f64], x: &[DVec3], parents: &[Option<usize>], p: DVec3) -> usize {
    let mut best: Option<(usize, f64)> = None;
    for i in 0..mu.len() {
        // The top of the hierarchy reaches everywhere.
        let soi = parents[i].map_or(f64::INFINITY, |q| {
            sphere_of_influence((x[i] - x[q]).length(), mu[i], mu[q])
        });
        if (p - x[i]).length() < soi && best.is_none_or(|(_, s)| soi < s) {
            best = Some((i, soi));
        }
    }
    best.map_or(0, |(i, _)| i)
}

/// The five Lagrange points of a pair (primary at `x1` with `mu1`, secondary at `x2` with
/// `mu2`), in inertial coordinates, for the pair's instantaneous configuration: the
/// equilibria of the frame turning with the line between them, about their barycentre.
/// `v1`, `v2` give the orbital plane (L4 leads the secondary, L5 trails it).
pub fn lagrange_points(
    (x1, v1, mu1): (DVec3, DVec3, f64),
    (x2, v2, mu2): (DVec3, DVec3, f64),
) -> [DVec3; 5] {
    let d = x2 - x1;
    let r = d.length();
    let ex = d / r;
    let h = d.cross(v2 - v1);
    let ez = h.normalize();
    let ey = ez.cross(ex); // direction of the secondary's motion
    let m = mu2 / (mu1 + mu2);
    let bary = x1 + d * m;
    // Collinear points: positions along ex (in units of r, from the barycentre) where
    // gravity of both and the centrifugal term balance:
    // f(u) = u − (1−m)(u+m)/|u+m|³ − m(u−1+m)/|u−1+m|³ = 0.
    let balance = |u: f64| {
        let (a, b) = (u + m, u - 1.0 + m);
        u - (1.0 - m) * a / (a.abs() * a * a) - m * b / (b.abs() * b * b)
    };
    let hill = det::cbrt(m / 3.0);
    let solve = |mut u: f64| {
        for _ in 0..100 {
            let f = balance(u);
            let h = 1e-7 * u.abs().max(1e-3);
            let slope = (balance(u + h) - balance(u - h)) / (2.0 * h);
            let step = f / slope;
            u -= step;
            if step.abs() < 1e-15 {
                break;
            }
        }
        u
    };
    let l1 = solve(1.0 - m - hill);
    let l2 = solve(1.0 - m + hill);
    let l3 = solve(-1.0 - m);
    let (s60, c60) = (0.75f64.sqrt(), 0.5);
    // L4/L5 form equilateral triangles with both bodies.
    let l4 = x1 + (ex * c60 + ey * s60) * r;
    let l5 = x1 + (ex * c60 - ey * s60) * r;
    [
        bary + ex * (l1 * r),
        bary + ex * (l2 * r),
        bary + ex * (l3 * r),
        l4,
        l5,
    ]
}

/// Axes of the frame turning with a pair: x towards the secondary, y along their orbital
/// angular momentum (the world is Y-up: a prograde orbit turns about +Y), z = x × y, which
/// points against the secondary's motion.
fn corotating_axes(x1: DVec3, v1: DVec3, x2: DVec3, v2: DVec3) -> (DVec3, DVec3, DVec3) {
    let d = x2 - x1;
    let ex = d.normalize();
    let ey = d.cross(v2 - v1).normalize();
    (ex, ey, ex.cross(ey))
}

/// Maps a point at time t into the frame turning with a pair at that time: origin at the
/// primary, axes from [`corotating_axes`] (for drawing trajectories in the co-rotating view,
/// where L-points stand still; L4, ahead of the secondary, has z < 0).
pub fn to_corotating(p: DVec3, (x1, v1): (DVec3, DVec3), (x2, v2): (DVec3, DVec3)) -> DVec3 {
    let (ex, ey, ez) = corotating_axes(x1, v1, x2, v2);
    let q = p - x1;
    DVec3::new(q.dot(ex), q.dot(ey), q.dot(ez))
}

/// Maps a co-rotating point back to inertial coordinates (the inverse of
/// [`to_corotating`] for the same pair state).
pub fn from_corotating(c: DVec3, (x1, v1): (DVec3, DVec3), (x2, v2): (DVec3, DVec3)) -> DVec3 {
    let (ex, ey, ez) = corotating_axes(x1, v1, x2, v2);
    x1 + ex * c.x + ey * c.y + ez * c.z
}

#[cfg(test)]
#[path = "analysis_tests.rs"]
mod tests;
