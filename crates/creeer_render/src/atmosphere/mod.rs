//! Physically based planetary atmosphere: single scattering by molecules (Rayleigh) and
//! aerosols (Mie), integrated numerically along view rays.
//!
//! This f64 CPU model computes the light that reaches the camera's surroundings (sun colour
//! after extinction, sky and ground ambient), so object lighting matches the sky drawn by
//! `sky.wgsl`, which evaluates the same equations per pixel.
//!
//! **Precision:** for a 500 km planet, the textbook ray–sphere formulas subtract numbers
//! around 2.5·10¹¹ m² to get results of a few m², which loses everything in f32 and much in
//! f64. All formulas here work with the *height above the surface* h instead, e.g. the
//! squared radius difference r² − R² is computed as h·(2R + h).

use creeer_math::det;
use creeer_math::glam::DVec3;
use std::f64::consts::PI;

/// Optical properties of an atmosphere. Lengths in metres, coefficients per metre.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Atmosphere {
    /// Radius of the solid surface (m).
    pub planet_radius: f64,
    /// Thickness of the simulated atmosphere above the surface (m).
    pub height: f64,
    /// Rayleigh scattering coefficients at the surface for R, G, B (1/m).
    pub rayleigh_scattering: [f64; 3],
    /// Height over which molecular density falls by a factor e (m).
    pub rayleigh_scale_height: f64,
    /// Mie (aerosol) scattering coefficient at the surface (1/m).
    pub mie_scattering: f64,
    /// Mie extinction (scattering + absorption) at the surface (1/m).
    pub mie_extinction: f64,
    /// Height over which aerosol density falls by a factor e (m).
    pub mie_scale_height: f64,
    /// Mie phase asymmetry g (0 = isotropic, → 1 = strongly forward).
    pub mie_g: f64,
    /// Diffuse reflectance of the ground, R, G, B.
    pub ground_albedo: [f64; 3],
}

impl Atmosphere {
    /// Earth-like air around a planet of the given radius: Rayleigh coefficients of Bruneton & Neyret
    /// (2008); aerosols of a clean continental day (optical depth 0.1 at 550 nm over a
    /// 1.2 km scale height, single-scattering albedo 0.9, asymmetry g = 0.76). The aerosol
    /// values in many papers (τ ≈ 0.005) describe exceptionally clean air and make the sky
    /// several times too dark.
    pub fn earthlike(planet_radius: f64) -> Self {
        Self {
            planet_radius,
            height: 80_000.0,
            rayleigh_scattering: [5.802e-6, 13.558e-6, 33.1e-6],
            rayleigh_scale_height: 8_000.0,
            mie_scattering: 7.5e-5,
            mie_extinction: 8.3e-5,
            mie_scale_height: 1_200.0,
            mie_g: 0.76,
            ground_albedo: [0.25, 0.27, 0.20],
        }
    }

    fn top_radius(&self) -> f64 {
        self.planet_radius + self.height
    }

    /// Extinction coefficient (1/m) per channel at height `h`.
    fn extinction(&self, h: f64) -> [f64; 3] {
        let rayleigh = det::exp(-h.max(0.0) / self.rayleigh_scale_height);
        let mie = det::exp(-h.max(0.0) / self.mie_scale_height) * self.mie_extinction;
        self.rayleigh_scattering.map(|beta| beta * rayleigh + mie)
    }
}

/// A point at height `h` above the surface looking along a direction whose cosine with the
/// local vertical is `mu`. All geometry below is expressed through (h, mu).
#[derive(Clone, Copy, Debug)]
struct Ray {
    h: f64,
    mu: f64,
}

impl Ray {
    fn radius(&self, atmosphere: &Atmosphere) -> f64 {
        atmosphere.planet_radius + self.h
    }

    /// Height above the surface after travelling `t` metres along the ray.
    fn height_at(&self, atmosphere: &Atmosphere, t: f64) -> f64 {
        let big_r = atmosphere.planet_radius;
        let r = self.radius(atmosphere);
        // r_t² − R² = h(2R + h) + t² + 2 r μ t, divided by (r_t + R) to get r_t − R.
        let numerator = self.h * (2.0 * big_r + self.h) + t * t + 2.0 * r * self.mu * t;
        let r_t = (r * r + t * t + 2.0 * r * self.mu * t).max(0.0).sqrt();
        numerator / (r_t + big_r)
    }

    /// Distance to the ground along the ray, if the ray hits the planet.
    fn ground_distance(&self, atmosphere: &Atmosphere) -> Option<f64> {
        let r = self.radius(atmosphere);
        // Discriminant r²μ² − (r² − R²) with r² − R² = h(2R + h).
        let discriminant =
            r * r * self.mu * self.mu - self.h * (2.0 * atmosphere.planet_radius + self.h);
        (self.mu < 0.0 && discriminant >= 0.0)
            .then(|| (-r * self.mu - discriminant.sqrt()).max(0.0))
    }

    /// Distances where the ray enters and leaves the atmosphere's outer shell.
    fn atmosphere_span(&self, atmosphere: &Atmosphere) -> Option<(f64, f64)> {
        let r = self.radius(atmosphere);
        let top_minus_r = atmosphere.height - self.h;
        // r_top² − r² = (r_top − r)(r_top + r).
        let discriminant = r * r * self.mu * self.mu + top_minus_r * (atmosphere.top_radius() + r);
        if discriminant < 0.0 {
            return None;
        }
        let root = discriminant.sqrt();
        let (enter, exit) = (-r * self.mu - root, -r * self.mu + root);
        (exit > 0.0).then_some((enter.max(0.0), exit))
    }
}

fn scale(v: [f64; 3], s: f64) -> [f64; 3] {
    v.map(|c| c * s)
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn mul(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] * b[0], a[1] * b[1], a[2] * b[2]]
}

/// Fraction of light per channel that survives a straight path from height `h` along the
/// direction with vertical cosine `mu` out of the atmosphere (zero if the planet blocks it).
pub fn transmittance_to_space(atmosphere: &Atmosphere, h: f64, mu: f64) -> [f64; 3] {
    const STEPS: u32 = 48;
    let ray = Ray { h, mu };
    if ray.ground_distance(atmosphere).is_some() {
        return [0.0; 3];
    }
    let Some((start, end)) = ray.atmosphere_span(atmosphere) else {
        return [1.0; 3];
    };
    let dt = (end - start) / f64::from(STEPS);
    let mut depth = [0.0; 3];
    for i in 0..STEPS {
        let t = start + (f64::from(i) + 0.5) * dt;
        depth = add(
            depth,
            scale(atmosphere.extinction(ray.height_at(atmosphere, t)), dt),
        );
    }
    depth.map(|d| det::exp(-d))
}

/// Rayleigh phase function for scattering angle cosine `nu`.
fn rayleigh_phase(nu: f64) -> f64 {
    3.0 / (16.0 * PI) * (1.0 + nu * nu)
}

/// Cornette–Shanks Mie phase function.
fn mie_phase(g: f64, nu: f64) -> f64 {
    let g2 = g * g;
    3.0 / (8.0 * PI) * (1.0 - g2) * (1.0 + nu * nu)
        / ((2.0 + g2) * det::pow(1.0 + g2 - 2.0 * g * nu, 1.5))
}

/// Radiance of the sky (single scattering plus the multiple-scattering approximation) seen
/// from height `h` (m) looking along `view`,
/// with `up` the local vertical and `to_sun` the unit vector towards the sun, for a sun of
/// irradiance `sun` at the top of the atmosphere.
pub fn sky_radiance(
    atmosphere: &Atmosphere,
    h: f64,
    up: DVec3,
    view: DVec3,
    to_sun: DVec3,
    sun: [f64; 3],
) -> [f64; 3] {
    const STEPS: u32 = 32;
    let ray = Ray {
        h,
        mu: view.dot(up),
    };
    let Some((start, mut end)) = ray.atmosphere_span(atmosphere) else {
        return [0.0; 3];
    };
    if let Some(ground) = ray.ground_distance(atmosphere) {
        end = end.min(ground);
    }
    if end <= start {
        return [0.0; 3];
    }
    let luts = atmosphere_luts(atmosphere);
    let r = ray.radius(atmosphere);
    let (mu_s, nu) = (up.dot(to_sun), view.dot(to_sun));
    let phase_r = rayleigh_phase(nu);
    let phase_m = mie_phase(atmosphere.mie_g, nu);

    let dt = (end - start) / f64::from(STEPS);
    let mut view_depth = [0.0; 3];
    let mut radiance = [0.0; 3];
    for i in 0..STEPS {
        let t = start + (f64::from(i) + 0.5) * dt;
        let h_t = ray.height_at(atmosphere, t);
        let extinction = atmosphere.extinction(h_t);
        let half = scale(extinction, dt * 0.5);
        view_depth = add(view_depth, half);
        // Sun direction relative to the local vertical at the sample point.
        let r_t = r + (h_t - h);
        let mu_s_t = (r * mu_s + t * nu) / r_t;
        let to_sun_t = transmittance_to_space(atmosphere, h_t, mu_s_t);
        let rayleigh_density = det::exp(-h_t.max(0.0) / atmosphere.rayleigh_scale_height);
        let mie_density = det::exp(-h_t.max(0.0) / atmosphere.mie_scale_height);
        let scattering = atmosphere.rayleigh_scattering.map(|beta| {
            beta * rayleigh_density * phase_r + atmosphere.mie_scattering * mie_density * phase_m
        });
        let to_camera = view_depth.map(|d| det::exp(-d));
        radiance = add(
            radiance,
            scale(mul(mul(scattering, to_sun_t), mul(to_camera, sun)), dt),
        );
        // Orders ≥ 2: isotropic source Ψ from the precomputed table.
        let multiple = mul(atmosphere.scattering(h_t), luts.multiscatter(h_t, mu_s_t));
        radiance = add(radiance, scale(mul(multiple, mul(to_camera, sun)), dt));
        view_depth = add(view_depth, half);
    }
    radiance
}

/// Light arriving near a point at height `h` (m): what objects there should be lit with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalLight {
    /// Sun irradiance after passing through the atmosphere (on a surface facing the sun).
    pub sun: [f64; 3],
    /// Cosine-weighted average radiance of the sky hemisphere.
    pub sky: [f64; 3],
    /// Average radiance of light bounced from the ground below.
    pub ground: [f64; 3],
}

/// Computes [`LocalLight`] by integrating the sky over the upper hemisphere.
pub fn local_light(
    atmosphere: &Atmosphere,
    h: f64,
    up: DVec3,
    to_sun: DVec3,
    sun: [f64; 3],
) -> LocalLight {
    let sun_at_point = mul(sun, transmittance_to_space(atmosphere, h, up.dot(to_sun)));
    // Cosine-weighted hemisphere: average of L·cosθ over a grid, normalised.
    let (tangent, bitangent) = orthonormal_basis(up);
    let (rings, segments) = (6u32, 12u32);
    let (mut sum, mut weight) = ([0.0; 3], 0.0);
    for ring in 0..rings {
        let cos_theta = 1.0 - (f64::from(ring) + 0.5) / f64::from(rings);
        let sin_theta = (1.0 - cos_theta * cos_theta).sqrt();
        for segment in 0..segments {
            let phi = 2.0 * PI * f64::from(segment) / f64::from(segments);
            let (sp, cp) = det::sin_cos(phi);
            let dir = up * cos_theta + (tangent * cp + bitangent * sp) * sin_theta;
            let l = sky_radiance(atmosphere, h, up, dir, to_sun, sun);
            sum = add(sum, scale(l, cos_theta));
            weight += cos_theta;
        }
    }
    let sky = scale(sum, 1.0 / weight);
    // Ground: Lambertian, lit by the sun and the sky (irradiance = π × sky radiance).
    let sun_on_ground = scale(sun_at_point, up.dot(to_sun).max(0.0));
    let irradiance = add(sun_on_ground, scale(sky, PI));
    let ground = mul(atmosphere.ground_albedo, scale(irradiance, 1.0 / PI));
    LocalLight {
        sun: sun_at_point,
        sky,
        ground,
    }
}

fn orthonormal_basis(n: DVec3) -> (DVec3, DVec3) {
    let reference = if n.y.abs() < 0.99 { DVec3::Y } else { DVec3::X };
    let tangent = reference.cross(n).normalize();
    (tangent, n.cross(tangent))
}

mod luts;
mod sky;
#[cfg(test)]
mod tests;

pub use luts::{AtmosphereLuts, MULTISCATTER_LUT, TRANSMITTANCE_LUT, atmosphere_luts};
pub use sky::Sky;
