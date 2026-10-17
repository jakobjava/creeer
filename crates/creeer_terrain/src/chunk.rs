//! Voxel chunks: blocks of 32³ cells of the terrain's density field, in a planet-centred
//! octree.
//!
//! A chunk of level `L` has cells of 2^L metres, so it is 32·2^L m wide; level 0 is the full
//! resolution (1 m cells, 32 m chunks). Chunks of one level tile space on a grid aligned with
//! the planet's axes, and each splits into the eight chunks of the next finer level.
//!
//! The density is negative inside rock and positive in air; the surface is its zero set.
//! A chunk samples it on a 35³ grid, sample `(i, j, k)` at `origin + (i−1, j−1, k−1)·cell`,
//! so it covers cells −1..=32 in each direction: one cell beyond each face. With that rim it
//! builds every quad reaching into its box without its neighbours, and clips them at the box
//! (`surface_nets.rs`), so neighbouring chunks of one level join without gaps.

use crate::edits::Edit;
use crate::terrain::{Terrain, generated};
use creeer_math::glam::{DVec3, IVec3};

/// Cells per chunk edge.
pub const CHUNK_CELLS: i32 = 32;
/// Samples per chunk edge: cells −1..=32 need samples −1..=33.
pub const SAMPLES: usize = 35;
/// Coarsest level (cells of 32 km; one chunk is about a whole 500 km planet).
pub const MAX_CHUNK_LEVEL: u8 = 15;

/// Identifies a chunk: octree level and grid position on that level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChunkKey {
    /// Level: cells of 2^level metres.
    pub level: u8,
    /// Grid position, in chunk widths from the planet centre.
    pub pos: IVec3,
}

impl Ord for ChunkKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.level, self.pos.to_array()).cmp(&(other.level, other.pos.to_array()))
    }
}

impl PartialOrd for ChunkKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl ChunkKey {
    /// Cell edge (m).
    pub fn cell_size(&self) -> f64 {
        f64::from(1u32 << self.level)
    }

    /// Chunk edge (m).
    pub fn size(&self) -> f64 {
        self.cell_size() * f64::from(CHUNK_CELLS)
    }

    /// Corner with the smallest coordinates (m, planet-centred).
    pub fn origin(&self) -> DVec3 {
        self.pos.as_dvec3() * self.size()
    }

    /// Centre (m).
    pub fn center(&self) -> DVec3 {
        self.origin() + DVec3::splat(0.5 * self.size())
    }

    /// The chunk of `level` containing point `p`.
    pub fn containing(p: DVec3, level: u8) -> Self {
        let size = f64::from(1u32 << level) * f64::from(CHUNK_CELLS);
        #[allow(clippy::cast_possible_truncation)] // planet-sized coordinates fit i32 chunks
        let pos = (p / size).floor().as_ivec3();
        Self { level, pos }
    }

    /// The chunk one level coarser that contains this one.
    pub fn parent(&self) -> Self {
        Self {
            level: self.level + 1,
            pos: self.pos.div_euclid(IVec3::splat(2)),
        }
    }

    /// The eight chunks one level finer.
    pub fn children(&self) -> [Self; 8] {
        assert!(self.level > 0, "level-0 chunks have no children");
        std::array::from_fn(|i| {
            let i = i32::try_from(i).expect("< 8");
            Self {
                level: self.level - 1,
                pos: self.pos * 2 + IVec3::new(i & 1, (i >> 1) & 1, (i >> 2) & 1),
            }
        })
    }

    /// A unique 64-bit id (level and position packed), e.g. to name GPU resources.
    pub fn id(&self) -> u64 {
        // 20 bits per coordinate hold ±524,288 chunks: 16,000 km at level 0.
        let pack = |v: i32| u64::from(v.cast_unsigned() & 0xf_ffff);
        (u64::from(self.level) << 60)
            | (pack(self.pos.x) << 40)
            | (pack(self.pos.y) << 20)
            | pack(self.pos.z)
    }

    /// Whether `other` lies inside this chunk (or is this chunk).
    pub fn contains(&self, other: &ChunkKey) -> bool {
        other.level <= self.level && {
            let shift = self.level - other.level;
            other.pos.map(|v| v >> shift) == self.pos
        }
    }

    /// Nearest and farthest distance of the chunk's box from the planet centre (m).
    pub fn radial_range(&self) -> (f64, f64) {
        let (lo, hi) = (self.origin(), self.origin() + DVec3::splat(self.size()));
        let nearest = DVec3::ZERO.clamp(lo, hi).length();
        let farthest = lo.abs().max(hi.abs()).length();
        (nearest, farthest)
    }

    /// Distance from `p` to the chunk's box (0 inside).
    pub fn distance_to(&self, p: DVec3) -> f64 {
        let (lo, hi) = (self.origin(), self.origin() + DVec3::splat(self.size()));
        (p - p.clamp(lo, hi)).length()
    }
}

/// The density samples of one chunk.
#[derive(Clone, Debug)]
pub struct ChunkSamples {
    /// Which chunk.
    pub key: ChunkKey,
    /// `SAMPLES³` densities (m), x fastest; index 0 is sample −1.
    values: Vec<f32>,
}

impl ChunkSamples {
    /// Density at sample `(i, j, k)`, each in 0..SAMPLES (sample −1 is index 0).
    pub fn at(&self, i: usize, j: usize, k: usize) -> f32 {
        self.values[i + SAMPLES * (j + SAMPLES * k)]
    }

    /// World position (m) of sample `(i, j, k)`.
    pub fn position(&self, i: usize, j: usize, k: usize) -> DVec3 {
        let offset = DVec3::new(index(i), index(j), index(k)) - DVec3::ONE;
        self.key.origin() + offset * self.key.cell_size()
    }

    /// Samples a chunk from a density function (tests, edits).
    pub fn from_fn(key: ChunkKey, mut density: impl FnMut(DVec3) -> f64) -> Self {
        let mut values = Vec::with_capacity(SAMPLES * SAMPLES * SAMPLES);
        let (origin, cell) = (key.origin(), key.cell_size());
        for k in 0..SAMPLES {
            for j in 0..SAMPLES {
                for i in 0..SAMPLES {
                    let offset = DVec3::new(index(i), index(j), index(k)) - DVec3::ONE;
                    #[allow(clippy::cast_possible_truncation)] // densities near the surface
                    values.push(density(origin + offset * cell) as f32);
                }
            }
        }
        Self { key, values }
    }

    /// Applies an edit to the samples within one cell of its brush (enough for every cell
    /// the brush surface crosses to see exact values; `edits.rs`).
    pub fn apply(&mut self, edit: &Edit) {
        let (lo, hi) = edit.brush().bounds();
        let (origin, cell) = (self.key.origin(), self.key.cell_size());
        // Sample index of a coordinate: (x − origin) / cell + 1, widened by one cell.
        let span = |lo: f64, hi: f64, o: f64| {
            let n = SAMPLES as f64;
            let first = ((lo - o) / cell).floor().clamp(0.0, n);
            let last = (((hi - o) / cell).ceil() + 3.0).clamp(0.0, n);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0..=SAMPLES
            let range = (first as usize)..(last as usize);
            range
        };
        let (xs, ys, zs) = (
            span(lo.x, hi.x, origin.x),
            span(lo.y, hi.y, origin.y),
            span(lo.z, hi.z, origin.z),
        );
        for k in zs {
            for j in ys.clone() {
                for i in xs.clone() {
                    let at = i + SAMPLES * (j + SAMPLES * k);
                    let p = self.position(i, j, k);
                    #[allow(clippy::cast_possible_truncation)]
                    let v = edit.apply(p, f64::from(self.values[at])) as f32;
                    self.values[at] = v;
                }
            }
        }
    }

    /// True if the surface passes through the chunk (some samples in rock, some in air).
    pub fn has_surface(&self) -> bool {
        let first = self.values[0] < 0.0;
        self.values.iter().any(|&v| (v < 0.0) != first)
    }
}

fn index(i: usize) -> f64 {
    f64::from(u32::try_from(i).expect("small index"))
}

/// What a chunk holds before it is sampled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Occupancy {
    /// Entirely above the terrain.
    Air,
    /// Entirely inside rock.
    Rock,
    /// May contain surface: sample it.
    Mixed,
}

impl Terrain {
    /// Whether a chunk can hold surface, from the height range of the tiles under it (no
    /// sampling). Conservative: `Mixed` whenever in doubt.
    pub fn occupancy(&self, key: ChunkKey) -> Occupancy {
        if !self.chunk_edits(key).is_empty() {
            return Occupancy::Mixed;
        }
        let (near, far) = key.radial_range();
        let datum = self.planet().datum;
        let elevation = self.heights().elevation();
        // Bicubic interpolation can overshoot its samples slightly; two cells of margin.
        let margin = 2.0 * key.cell_size();
        if near > datum.equatorial_radius() + elevation.max_height() + margin {
            return Occupancy::Air;
        }
        if far < datum.polar_radius + elevation.min_height() - margin {
            return Occupancy::Rock;
        }
        // The datum may peak between the corners (a chunk across the equator) by up to
        // bulge·(size/R)²/4; four times that is a safe margin.
        let bulge = datum.equatorial_radius() - datum.polar_radius;
        let ratio = key.size() / datum.polar_radius;
        let between = bulge * ratio * ratio;
        // The tiles under the chunk's corners cover its footprint (a tile is wider than the
        // chunk), so their height range bounds the surface inside it.
        let mut sampler = self.heights().sampler(key.cell_size());
        let (mut lo, mut hi) = (f64::MAX, f64::MIN);
        for corner in 0..8 {
            let c = DVec3::new(
                f64::from(corner & 1),
                f64::from((corner >> 1) & 1),
                f64::from((corner >> 2) & 1),
            );
            let corner = key.origin() + c * key.size();
            let (tile, _) = sampler.tile(corner);
            let sea = datum.radius(corner);
            lo = lo.min(sea + tile.min);
            hi = hi.max(sea + tile.max);
        }
        if near > hi + between + margin {
            Occupancy::Air
        } else if far < lo - between - margin {
            Occupancy::Rock
        } else {
            Occupancy::Mixed
        }
    }

    /// The density samples of a chunk, edits included.
    pub fn sample_chunk(&self, key: ChunkKey) -> ChunkSamples {
        let mut sampler = self.heights().sampler(key.cell_size());
        let datum = self.planet().datum;
        let mut samples = ChunkSamples::from_fn(key, |p| generated(&datum, &mut sampler, p));
        for edit in &self.chunk_edits(key) {
            samples.apply(edit);
        }
        samples
    }

    /// The edits a chunk resolves, in order (`Edits::affecting`).
    pub fn chunk_edits(&self, key: ChunkKey) -> Vec<Edit> {
        let (lo, hi) = (key.origin(), key.origin() + DVec3::splat(key.size()));
        let cell = key.cell_size();
        self.edits().affecting(lo - cell, hi + cell, cell)
    }
}

/// A fast hasher for chunk keys (the default SipHash is several times slower, and chunk
/// lookups are on the frame's critical path).
#[derive(Clone, Copy, Debug, Default)]
pub struct KeyHasher(u64);

impl std::hash::Hasher for KeyHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_u64(u64::from(b));
        }
    }

    fn write_u64(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(0x517c_c1b7_2722_0a95);
    }

    fn write_u32(&mut self, word: u32) {
        self.write_u64(u64::from(word));
    }

    fn write_i32(&mut self, word: i32) {
        self.write_u64(u64::from(word.cast_unsigned()));
    }

    fn write_u8(&mut self, word: u8) {
        self.write_u64(u64::from(word));
    }
}

/// A map keyed by chunk.
pub type KeyMap<V> =
    std::collections::HashMap<ChunkKey, V, std::hash::BuildHasherDefault<KeyHasher>>;
/// A set of chunks.
pub type KeySet = std::collections::HashSet<ChunkKey, std::hash::BuildHasherDefault<KeyHasher>>;
