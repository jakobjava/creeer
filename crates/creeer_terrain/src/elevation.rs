//! Surface elevation: height above the planet's datum radius as a function of direction.
//!
//! The elevation is a sum of noise layers on the sphere (3D noise of the surface point, so
//! there are no seams or pole artefacts):
//! - **continents:** low-frequency noise mapped through a piecewise-linear curve, giving
//!   ocean basins, shelves, plains and highlands;
//! - **layers:** fractal noise (`Fbm`, ±amplitude) or ridged noise (`Ridged`, 0..amplitude,
//!   sharp crests), each optionally confined by a low-frequency mask and to land.
//!
//! Every evaluation is **band-limited** to the sample spacing it will be stored at: octaves
//! shorter than two samples are left out (they would alias into noise), and the octave
//! just above that fades in, so changing resolution never makes features pop.

use crate::noise::Perlin;
use creeer_math::glam::DVec3;

/// Shape of a noise layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
pub enum LayerKind {
    /// Fractal Brownian motion: rolling relief, ±amplitude.
    Fbm,
    /// Ridged multifractal: sharp crests and smooth valleys, 0..amplitude.
    Ridged,
}

/// A fractal noise in SI units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Octaves {
    /// Wavelength of the first octave (m).
    pub wavelength: f64,
    /// Number of octaves, each half the wavelength of the previous one.
    pub octaves: u32,
    /// Amplitude ratio between octaves (0.5 for classic 1/f relief).
    pub gain: f64,
}

/// Where a layer applies: a smooth step of a low-frequency noise from `from` to `to`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mask {
    /// The mask noise.
    pub noise: Octaves,
    /// Noise value where the layer starts.
    pub from: f64,
    /// Noise value where the layer has full strength.
    pub to: f64,
}

/// One relief layer.
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    /// Name (for diagnostics).
    pub name: String,
    /// Noise shape.
    pub kind: LayerKind,
    /// Its octaves.
    pub noise: Octaves,
    /// Height (m): ±amplitude for `Fbm`, up to amplitude for `Ridged`.
    pub amplitude: f64,
    /// Optional mask.
    pub mask: Option<Mask>,
    /// Fade the layer out below sea level (datum).
    pub land_only: bool,
}

/// The planet's elevation model.
///
/// Low-frequency noise on a single planet is lopsided (a 900 km wavelength fits about once),
/// so the continent and mask values are not used raw: each is replaced by the fraction of
/// the planet's surface where it is lower. The data then says what it means: a coast at 0.54
/// gives 46 % land, a mask from 0.6 to 0.8 puts mountains on the top 40 %, for every seed.
#[derive(Clone, Debug)]
pub struct Elevation {
    /// Datum radius (m); heights are relative to it.
    pub radius: f64,
    /// Continent noise.
    pub continents: Octaves,
    /// (surface fraction with lower continent value, height in m), ascending.
    pub curve: Vec<(f64, f64)>,
    /// Relief layers.
    pub layers: Vec<Layer>,
    noises: Vec<Perlin>,
    /// Surface-fraction maps: continents, then each layer's mask.
    cdfs: Vec<Cdf>,
}

impl Elevation {
    /// The model; `seed` derives one independent noise per layer and mask.
    pub fn new(
        radius: f64,
        seed: u64,
        continents: Octaves,
        curve: Vec<(f64, f64)>,
        layers: Vec<Layer>,
    ) -> Self {
        // Noise 0: continents; 2i+1: layer i; 2i+2: its mask.
        let noises: Vec<Perlin> = (0..=2 * layers.len() as u64)
            .map(|i| Perlin::new(seed.wrapping_mul(0x2545_f491_4f6c_dd1d).wrapping_add(i)))
            .collect();
        let measure = |noise: &Perlin, o: Octaves| {
            Cdf::new(fibonacci_sphere(CDF_SAMPLES).map(|d| fractal(noise, d * radius, o, 0.0)))
        };
        let mut cdfs = vec![measure(&noises[0], continents)];
        for (i, layer) in layers.iter().enumerate() {
            if let Some(mask) = layer.mask {
                cdfs.push(measure(&noises[2 * i + 2], mask.noise));
            } else {
                cdfs.push(Cdf::default());
            }
        }
        Self {
            radius,
            continents,
            curve,
            layers,
            noises,
            cdfs,
        }
    }

    /// Height above the datum (m) at unit direction `dir`, without detail shorter than two
    /// samples at `spacing` metres.
    pub fn height(&self, dir: DVec3, spacing: f64) -> f64 {
        let p = dir * self.radius;
        let base = piecewise_linear(&self.curve, self.continent(dir, spacing));
        // Land fades in over the first 50 m above the datum.
        let land = smoothstep(-50.0, 50.0, base);
        let mut h = base;
        for (i, layer) in self.layers.iter().enumerate() {
            let mut weight = if layer.land_only { land } else { 1.0 };
            if let Some(mask) = layer.mask
                && weight > 0.0
            {
                let m = fractal(&self.noises[2 * i + 2], p, mask.noise, spacing);
                weight *= smoothstep(mask.from, mask.to, self.cdfs[i + 1].fraction_below(m));
            }
            if weight <= 0.0 {
                continue;
            }
            let noise = &self.noises[2 * i + 1];
            let value = match layer.kind {
                LayerKind::Fbm => fractal(noise, p, layer.noise, spacing),
                LayerKind::Ridged => ridged(noise, p, layer.noise, spacing),
            };
            h += weight * layer.amplitude * value;
        }
        h
    }

    /// The continent value at unit direction `dir`: the fraction of the planet's surface with
    /// a lower one (0..1; the curve maps it to height).
    pub fn continent(&self, dir: DVec3, spacing: f64) -> f64 {
        let c = fractal(&self.noises[0], dir * self.radius, self.continents, spacing);
        self.cdfs[0].fraction_below(c)
    }

    /// Largest height the model can produce (m), for culling.
    pub fn max_height(&self) -> f64 {
        let top = self.curve.iter().map(|&(_, h)| h).fold(f64::MIN, f64::max);
        top + self.layers.iter().map(|l| l.amplitude).sum::<f64>()
    }

    /// Lowest height the model can produce (m), for culling.
    pub fn min_height(&self) -> f64 {
        let bottom = self.curve.iter().map(|&(_, h)| h).fold(f64::MAX, f64::min);
        bottom
            - self
                .layers
                .iter()
                .filter(|l| l.kind == LayerKind::Fbm && !l.land_only)
                .map(|l| l.amplitude)
                .sum::<f64>()
    }
}

/// Samples used to measure a noise's distribution over the planet.
const CDF_SAMPLES: u32 = 8192;
/// Knots of the measured distribution (quantiles 0, 1/32, ..., 1); each segment spans 256
/// samples, so its slope is accurate to about 6 % and the height shows no terraces.
const CDF_KNOTS: usize = 33;

/// The distribution of a noise over the planet's surface, as a piecewise-linear map from
/// value to the fraction of the surface below it.
#[derive(Clone, Debug, Default)]
struct Cdf {
    /// Values at quantiles 0, 1/(n−1), ..., 1.
    knots: Vec<f64>,
}

impl Cdf {
    fn new(values: impl Iterator<Item = f64>) -> Self {
        let mut sorted: Vec<f64> = values.collect();
        sorted.sort_by(f64::total_cmp);
        let last = sorted.len() - 1;
        let knots = (0..CDF_KNOTS)
            .map(|k| sorted[k * last / (CDF_KNOTS - 1)])
            .collect();
        Self { knots }
    }

    /// Fraction of the surface with a value below `x` (0..1).
    fn fraction_below(&self, x: f64) -> f64 {
        let k = &self.knots;
        if x <= k[0] {
            return 0.0;
        }
        let i = k.partition_point(|&v| v <= x);
        if i >= k.len() {
            return 1.0;
        }
        let (a, b) = (k[i - 1], k[i]);
        let t = if b > a { (x - a) / (b - a) } else { 0.0 };
        #[allow(clippy::cast_precision_loss)] // small counts
        let n = (k.len() - 1) as f64;
        #[allow(clippy::cast_precision_loss)]
        let below = (i - 1) as f64;
        (below + t) / n
    }
}

/// `n` directions spread evenly over the sphere (Fibonacci lattice).
pub fn fibonacci_sphere(n: u32) -> impl Iterator<Item = DVec3> {
    let golden = std::f64::consts::PI * (3.0 - 5f64.sqrt());
    (0..n).map(move |i| {
        let i = f64::from(i);
        let z = 1.0 - 2.0 * (i + 0.5) / f64::from(n);
        let r = (1.0 - z * z).sqrt();
        let (s, c) = creeer_math::det::sin_cos(golden * i);
        DVec3::new(r * c, z, r * s)
    })
}

/// Weight of an octave of wavelength `wavelength` sampled every `spacing` metres: 0 below
/// two samples per wavelength, 1 from four, linear in between.
fn band_weight(wavelength: f64, spacing: f64) -> f64 {
    (wavelength / (2.0 * spacing) - 1.0).clamp(0.0, 1.0)
}

/// Octave `k`'s offset, so the lattices of different octaves do not line up.
fn octave_offset(k: u32) -> DVec3 {
    let k = f64::from(k);
    DVec3::new(17.13 * k + 0.37, 31.71 * k + 0.61, 47.29 * k + 0.83)
}

/// Normalised fractal Brownian motion at `p` (m), roughly [−1, 1].
pub(crate) fn fractal(noise: &Perlin, p: DVec3, o: Octaves, spacing: f64) -> f64 {
    // Normalised by every octave the layer has, so leaving octaves out does not rescale it.
    let full: f64 = (0..o.octaves)
        .fold((0.0, 1.0), |(sum, a), _| (sum + a, a * o.gain))
        .0;
    let (mut sum, mut amplitude, mut wavelength) = (0.0, 1.0, o.wavelength);
    for k in 0..o.octaves {
        let w = band_weight(wavelength, spacing);
        if w <= 0.0 {
            break; // every later octave is shorter still
        }
        let q = p / wavelength + octave_offset(k);
        sum += w * amplitude * noise.sample(q.x, q.y, q.z);
        amplitude *= o.gain;
        wavelength *= 0.5;
    }
    sum / full
}

/// Ridged multifractal at `p` (m), in [0, 1]: crests where the noise crosses zero, each octave
/// weighted by the previous one so detail gathers on the crests (Musgrave 1994).
fn ridged(noise: &Perlin, p: DVec3, o: Octaves, spacing: f64) -> f64 {
    let (mut sum, mut full, mut amplitude, mut wavelength) = (0.0, 0.0, 1.0, o.wavelength);
    let mut weight = 1.0;
    for k in 0..o.octaves {
        full += amplitude;
        let w = band_weight(wavelength, spacing);
        if w > 0.0 {
            let q = p / wavelength + octave_offset(k);
            let ridge = 1.0 - noise.sample(q.x, q.y, q.z).abs();
            let signal = ridge * ridge * weight;
            sum += w * amplitude * signal;
            weight = (signal * 2.0).clamp(0.0, 1.0);
        }
        amplitude *= o.gain;
        wavelength *= 0.5;
    }
    sum / full
}

/// Linear interpolation through `(x, y)` points sorted by x; constant beyond the ends.
fn piecewise_linear(points: &[(f64, f64)], x: f64) -> f64 {
    let Some(&(x0, y0)) = points.first() else {
        return 0.0;
    };
    if x <= x0 {
        return y0;
    }
    for pair in points.windows(2) {
        let ((xa, ya), (xb, yb)) = (pair[0], pair[1]);
        if x <= xb {
            return ya + (yb - ya) * (x - xa) / (xb - xa);
        }
    }
    points[points.len() - 1].1
}

/// Smooth step from 0 at `a` to 1 at `b` (C1).
fn smoothstep(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
