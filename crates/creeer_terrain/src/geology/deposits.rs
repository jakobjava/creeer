//! Ore deposits: where they are and what is inside them.
//!
//! Each deposit kind divides the planet into cube-sphere cells about its `cell` size wide.
//! A cell holds at most one deposit of the kind; whether it does, where, how large and how
//! it is oriented follow from a hash of (seed, kind, cell), so deposits need no storage
//! and are identical everywhere. A deposit is an ellipsoidal lens, horizontal in the local
//! frame of its centre, with a noisy outline. It is never larger than 0.45 of its cell, so a
//! point can only lie in deposits of its own cell and the cells around it.

use super::Geology;
use crate::cube::CubeCoord;
use crate::tiles::{TILE_CELLS, TileKey, level_spacing};
use creeer_math::det;
use creeer_math::glam::DVec3;

/// A kind of deposit, from the geology file.
#[derive(Clone, Debug)]
pub struct DepositKind {
    /// Name.
    pub name: String,
    /// The ore rock (material id).
    pub rock: u16,
    /// Provinces it forms in (indices).
    pub provinces: Vec<usize>,
    /// Surface elevation range at its centre (m).
    pub elevation: (f64, f64),
    /// Range of |latitude| at its centre (rad).
    pub latitude: (f64, f64),
    /// Cube-sphere level of its cells.
    pub level: u8,
    /// Probability that a cell holds one.
    pub chance: f64,
    /// Horizontal radius range (m).
    pub size: (f64, f64),
    /// Full vertical thickness range (m).
    pub thickness: (f64, f64),
    /// Depth range of its centre below the surface (m).
    pub depth: (f64, f64),
    /// Relative roughness of its outline.
    pub irregularity: f64,
}

/// One deposit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Deposit {
    /// Index of its kind.
    pub kind: u16,
    /// Centre (m, planet-centred).
    pub center: DVec3,
    /// Local axes: two horizontal, then up.
    pub axes: [DVec3; 3],
    /// Radii along the axes (m).
    pub radii: DVec3,
    /// Outline roughness.
    pub irregularity: f64,
    /// Offset of its outline noise.
    noise_offset: DVec3,
}

/// Where a point lies in a deposit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DepositHit {
    /// Index of the deposit's kind.
    pub kind: u16,
    /// Grade relative to the ore rock's average: 2.5 at the core, 0 at the rim (mean 1).
    pub grade: f64,
}

impl Deposit {
    /// Radius of a ball around the centre that contains the deposit (m).
    pub fn bounding_radius(&self) -> f64 {
        self.radii.max_element() * (1.0 + self.irregularity)
    }

    /// The deposit at `p`, if `p` is inside.
    pub fn hit(&self, p: DVec3, geology: &Geology) -> Option<DepositHit> {
        let d = p - self.center;
        let q = DVec3::new(
            d.dot(self.axes[0]) / self.radii.x,
            d.dot(self.axes[1]) / self.radii.y,
            d.dot(self.axes[2]) / self.radii.z,
        );
        let rho2 = q.length_squared();
        let limit = (1.0 + self.irregularity) * (1.0 + self.irregularity);
        if rho2 >= limit {
            return None;
        }
        // Rough outline: noise at a quarter of the horizontal size.
        let n = geology.noise_at(d / (0.25 * self.radii.x) + self.noise_offset);
        let rho = rho2.sqrt() * (1.0 + self.irregularity * n);
        (rho < 1.0).then_some(DepositHit {
            kind: self.kind,
            grade: 2.5 * (1.0 - rho * rho),
        })
    }
}

/// SplitMix64: a small, good hash-based random sequence.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1).
    fn unit(&mut self) -> f64 {
        #[allow(clippy::cast_precision_loss)] // 53 random bits
        let x = (self.next() >> 11) as f64;
        x / (1u64 << 53) as f64
    }

    fn between(&mut self, (lo, hi): (f64, f64)) -> f64 {
        lo + (hi - lo) * self.unit()
    }
}

/// East and north at a unit direction (any perpendicular pair at the poles).
pub fn tangent_frame(up: DVec3) -> (DVec3, DVec3) {
    let east = DVec3::Y.cross(up);
    let east = if east.length_squared() > 1e-12 {
        east.normalize()
    } else {
        DVec3::X
    };
    (east, up.cross(east))
}

impl Geology {
    /// The deposit of kind `kind` in cell `key`, if there is one.
    pub(super) fn generate_deposit(&self, kind: u16, key: TileKey) -> Option<Deposit> {
        let k = &self.deposits[usize::from(kind)];
        let cell_hash = (u64::from(key.face) << 58)
            ^ (u64::from(key.level) << 52)
            ^ (u64::from(key.i) << 26)
            ^ u64::from(key.j);
        let mut rng =
            Rng(self.seed ^ (u64::from(kind) << 40) ^ cell_hash.wrapping_mul(0x2545_f491));
        rng.next();
        if rng.unit() >= k.chance {
            return None;
        }
        let size = 2.0 / f64::from(1u32 << key.level);
        let c = CubeCoord {
            face: key.face,
            s: -1.0 + (f64::from(key.i) + rng.unit()) * size,
            t: -1.0 + (f64::from(key.j) + rng.unit()) * size,
        };
        let up = c.direction();
        let latitude = det::asin(up.y.clamp(-1.0, 1.0)).abs();
        if latitude < k.latitude.0 || latitude > k.latitude.1 {
            return None;
        }
        let height = self.elevation_at(up);
        if height < k.elevation.0 || height > k.elevation.1 {
            return None;
        }
        if !k.provinces.is_empty() && !k.provinces.contains(&self.province_at(up, height)) {
            return None;
        }
        let depth = rng.between(k.depth);
        let a = rng.between(k.size);
        let b = a * rng.between((0.4, 1.0));
        let thickness = rng.between(k.thickness);
        let (east, north) = tangent_frame(up);
        let (sin, cos) = det::sin_cos(rng.unit() * std::f64::consts::TAU);
        Some(Deposit {
            kind,
            center: up * (self.datum.radius(up) + height - depth),
            axes: [east * cos + north * sin, north * cos - east * sin, up],
            radii: DVec3::new(a, b, 0.5 * thickness),
            irregularity: k.irregularity,
            noise_offset: DVec3::new(rng.unit(), rng.unit(), rng.unit()) * 200.0,
        })
    }

    /// Every deposit that may reach within `reach` metres of `p`.
    pub fn deposits_near(&self, p: DVec3, reach: f64) -> Vec<Deposit> {
        let up = p.normalize();
        let (east, north) = tangent_frame(up);
        let mut found = Vec::new();
        for (kind, k) in self.deposits.iter().enumerate() {
            let kind = u16::try_from(kind).expect("few kinds");
            // Step by less than the smallest tile width, so no neighbouring cell is skipped.
            let step = 0.7 * level_spacing(1.0, k.level) * TILE_CELLS as f64 * 0.75;
            let mut keys: Vec<TileKey> = Vec::with_capacity(25);
            for du in -2..=2 {
                for dv in -2..=2 {
                    let d = up + (east * f64::from(du) + north * f64::from(dv)) * step;
                    let key = TileKey::containing(CubeCoord::from_direction(d), k.level);
                    if !keys.contains(&key) {
                        keys.push(key);
                    }
                }
            }
            for key in keys {
                if let Some(deposit) = self.cached_deposit(kind, key)
                    && (deposit.center - p).length() < deposit.bounding_radius() + reach
                {
                    found.push(deposit);
                }
            }
        }
        found
    }

    fn cached_deposit(&self, kind: u16, key: TileKey) -> Option<Deposit> {
        let cache = self
            .deposit_cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(&d) = cache.get(&(kind, key)) {
            return d;
        }
        drop(cache);
        let deposit = self.generate_deposit(kind, key);
        let mut cache = self
            .deposit_cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if cache.len() > 200_000 {
            cache.clear();
        }
        cache.insert((kind, key), deposit);
        deposit
    }
}
