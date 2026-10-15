//! Precomputed tables (Hillaire 2020): transmittance and multiple scattering.

use super::*;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

// ---------------------------------------------------------------------------------------

/// Transmittance table size: (view-angle axis, altitude axis).
pub const TRANSMITTANCE_LUT: (usize, usize) = (256, 64);
/// Multiple-scattering table size (sun-angle axis × altitude axis).
pub const MULTISCATTER_LUT: usize = 32;
/// Directions used to integrate light arriving from the whole sphere.
const MULTISCATTER_DIRECTIONS: u32 = 64;
const MULTISCATTER_STEPS: u32 = 20;

impl Atmosphere {
    /// √(r_top² − R²): distance from the surface to the top along the horizon tangent.
    fn horizon_extent(&self) -> f64 {
        (self.height * (2.0 * self.planet_radius + self.height)).sqrt()
    }

    /// Scattering coefficient (Rayleigh + Mie) per channel at height `h` (1/m).
    pub(super) fn scattering(&self, h: f64) -> [f64; 3] {
        let rayleigh = det::exp(-h.max(0.0) / self.rayleigh_scale_height);
        let mie = det::exp(-h.max(0.0) / self.mie_scale_height) * self.mie_scattering;
        self.rayleigh_scattering.map(|beta| beta * rayleigh + mie)
    }
}

/// Bruneton's horizon-aware mapping from (height, view cosine) to table coordinates in
/// [0, 1]². It spends resolution near the horizon, where transmittance changes fastest.
/// Only rays that do not hit the ground are represented.
pub(super) fn transmittance_uv(a: &Atmosphere, h: f64, mu: f64) -> (f64, f64) {
    let extent = a.horizon_extent();
    let h = h.clamp(0.0, a.height);
    let rho = (h * (2.0 * a.planet_radius + h)).sqrt();
    let r = a.planet_radius + h;
    let top_minus_r = a.height - h;
    let discriminant = (r * r * mu * mu + top_minus_r * (a.top_radius() + r)).max(0.0);
    let d = (-r * mu + discriminant.sqrt()).max(0.0);
    let (d_min, d_max) = (top_minus_r, rho + extent);
    ((d - d_min) / (d_max - d_min), rho / extent)
}

/// Inverse of [`transmittance_uv`].
pub(super) fn transmittance_params(a: &Atmosphere, u: f64, v: f64) -> (f64, f64) {
    let extent = a.horizon_extent();
    let rho = extent * v;
    let big_r = a.planet_radius;
    // h = √(ρ² + R²) − R without cancellation.
    let h = rho * rho / ((rho * rho + big_r * big_r).sqrt() + big_r);
    let r = big_r + h;
    let (d_min, d_max) = (a.height - h, rho + extent);
    let d = d_min + u * (d_max - d_min);
    let mu = if d <= 0.0 {
        1.0
    } else {
        // r_top² = r² + d² + 2rdμ, with r_top² − r² = extent² − ρ².
        ((extent * extent - rho * rho - d * d) / (2.0 * r * d)).clamp(-1.0, 1.0)
    };
    (h, mu)
}

/// Bilinear lookup in a row-major RGB table; (u, v) in [0, 1] map to the first/last texel
/// centres (the GPU uses the same convention).
fn sample_table(table: &[[f32; 3]], width: usize, height: usize, u: f64, v: f64) -> [f64; 3] {
    let index = |x: f64, n: usize| {
        #[allow(clippy::cast_precision_loss)]
        let f = x.clamp(0.0, 1.0) * (n - 1) as f64;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let i = (f.floor() as usize).min(n - 2);
        #[allow(clippy::cast_precision_loss)]
        let frac = f - i as f64;
        (i, frac)
    };
    let ((x, fx), (y, fy)) = (index(u, width), index(v, height));
    let at = |x: usize, y: usize| table[y * width + x].map(f64::from);
    let lerp = |a: [f64; 3], b: [f64; 3], t: f64| [0, 1, 2].map(|c| a[c] + (b[c] - a[c]) * t);
    lerp(
        lerp(at(x, y), at(x + 1, y), fx),
        lerp(at(x, y + 1), at(x + 1, y + 1), fx),
        fy,
    )
}

/// Points spread evenly over the unit sphere (Fibonacci lattice).
fn fibonacci_sphere(i: u32, n: u32) -> DVec3 {
    let golden = PI * (3.0 - 5f64.sqrt());
    let y = 1.0 - 2.0 * (f64::from(i) + 0.5) / f64::from(n);
    let radius = (1.0 - y * y).sqrt();
    let (s, c) = det::sin_cos(golden * f64::from(i));
    DVec3::new(c * radius, y, s * radius)
}

/// The precomputed tables of one atmosphere.
#[derive(Debug)]
pub struct AtmosphereLuts {
    /// The atmosphere these tables belong to.
    pub atmosphere: Atmosphere,
    /// Transmittance to space, RGB, `TRANSMITTANCE_LUT.0` × `TRANSMITTANCE_LUT.1`, row-major.
    pub transmittance: Vec<[f32; 3]>,
    /// Multiple-scattering source Ψ per unit sun irradiance, RGB, row-major (altitude rows).
    pub multiscatter: Vec<[f32; 3]>,
}

impl AtmosphereLuts {
    /// Builds both tables (tens of milliseconds; cached by [`atmosphere_luts`]).
    pub fn build(atmosphere: &Atmosphere) -> Self {
        let (w, h) = TRANSMITTANCE_LUT;
        #[allow(clippy::cast_possible_truncation)] // table values are in [0, 1]
        let transmittance = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .map(|(x, y)| {
                let (u, v) = (ratio(x, w), ratio(y, h));
                let (height, mu) = transmittance_params(atmosphere, u, v);
                transmittance_to_space(atmosphere, height, mu).map(|t| t as f32)
            })
            .collect();
        let mut luts = Self {
            atmosphere: *atmosphere,
            transmittance,
            multiscatter: Vec::new(),
        };
        let n = MULTISCATTER_LUT;
        #[allow(clippy::cast_possible_truncation)] // radiance values are small
        let multiscatter = (0..n)
            .flat_map(|y| (0..n).map(move |x| (x, y)))
            .map(|(x, y)| {
                let mu_s = ratio(x, n) * 2.0 - 1.0;
                let height = ratio(y, n) * atmosphere.height;
                luts.compute_multiscatter(height, mu_s).map(|v| v as f32)
            })
            .collect();
        luts.multiscatter = multiscatter;
        luts
    }

    /// Transmittance to space from height `h` along vertical cosine `mu` (0 if the planet
    /// is in the way).
    pub fn transmittance(&self, h: f64, mu: f64) -> [f64; 3] {
        if (Ray { h, mu }).ground_distance(&self.atmosphere).is_some() {
            return [0.0; 3];
        }
        let (u, v) = transmittance_uv(&self.atmosphere, h, mu);
        let (w, height) = TRANSMITTANCE_LUT;
        sample_table(&self.transmittance, w, height, u, v)
    }

    /// Multiple-scattering source Ψ at height `h` for sun cosine `mu_s` (per unit sun).
    pub fn multiscatter(&self, h: f64, mu_s: f64) -> [f64; 3] {
        let n = MULTISCATTER_LUT;
        let u = mu_s * 0.5 + 0.5;
        let v = (h / self.atmosphere.height).clamp(0.0, 1.0);
        sample_table(&self.multiscatter, n, n, u, v)
    }

    /// Ψ_ms = L₂ / (1 − f_ms): second-order scattered light L₂ arriving from all directions,
    /// summed over all higher orders as a geometric series with transfer factor f_ms.
    fn compute_multiscatter(&self, h: f64, mu_s: f64) -> [f64; 3] {
        let a = &self.atmosphere;
        let to_sun = DVec3::new((1.0 - mu_s * mu_s).max(0.0).sqrt(), mu_s, 0.0);
        let r = a.planet_radius + h;
        let isotropic = 1.0 / (4.0 * PI);
        let (mut second_order, mut transfer) = ([0.0; 3], [0.0; 3]);
        for k in 0..MULTISCATTER_DIRECTIONS {
            let dir = fibonacci_sphere(k, MULTISCATTER_DIRECTIONS);
            let ray = Ray { h, mu: dir.y };
            let Some((start, mut end)) = ray.atmosphere_span(a) else {
                continue;
            };
            let ground = ray.ground_distance(a);
            if let Some(g) = ground {
                end = end.min(g);
            }
            let nu = dir.dot(to_sun);
            let dt = (end - start) / f64::from(MULTISCATTER_STEPS);
            let mut throughput = [1.0; 3];
            for s in 0..MULTISCATTER_STEPS {
                let t = start + (f64::from(s) + 0.5) * dt;
                let h_t = ray.height_at(a, t);
                let (scattering, extinction) = (a.scattering(h_t), a.extinction(h_t));
                let mu_s_t = (r * mu_s + t * nu) / (r + (h_t - h));
                let sun = self.transmittance(h_t, mu_s_t);
                for c in 0..3 {
                    // Exact integral of the throughput over the segment.
                    let segment = det::exp(-extinction[c] * dt);
                    let weight = throughput[c] * (1.0 - segment) / extinction[c];
                    second_order[c] += weight * scattering[c] * sun[c] * isotropic;
                    transfer[c] += weight * scattering[c];
                    throughput[c] *= segment;
                }
            }
            if let Some(g) = ground {
                let mu_s_g = (r * mu_s + g * nu) / (r + (ray.height_at(a, g) - h));
                let sun = self.transmittance(0.0, mu_s_g);
                for c in 0..3 {
                    second_order[c] +=
                        throughput[c] * sun[c] * mu_s_g.max(0.0) * a.ground_albedo[c] / PI;
                }
            }
        }
        let n = f64::from(MULTISCATTER_DIRECTIONS);
        [0, 1, 2].map(|c| (second_order[c] / n) / (1.0 - transfer[c] / n))
    }
}

fn ratio(i: usize, n: usize) -> f64 {
    #[allow(clippy::cast_precision_loss)] // small table sizes
    let r = i as f64 / (n - 1) as f64;
    r
}

/// The tables for `atmosphere`, built on first use and cached (the last few atmospheres).
pub fn atmosphere_luts(atmosphere: &Atmosphere) -> Arc<AtmosphereLuts> {
    static CACHE: OnceLock<Mutex<Vec<Arc<AtmosphereLuts>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(Vec::new()));
    let mut cache = cache.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(luts) = cache.iter().find(|l| l.atmosphere == *atmosphere) {
        return Arc::clone(luts);
    }
    let luts = Arc::new(AtmosphereLuts::build(atmosphere));
    if cache.len() >= 4 {
        cache.remove(0);
    }
    cache.push(Arc::clone(&luts));
    luts
}
