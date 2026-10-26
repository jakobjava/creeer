//! Height tiles: the elevation sampled on cube-sphere grids, one quadtree per face.
//!
//! Evaluating the elevation costs about 30 noise samples (all layers and octaves), and a
//! 32³ voxel chunk needs 36,000 heights. Storing heights on a 2D grid and interpolating
//! instead makes chunk generation an order of magnitude cheaper; the grid is a cube-sphere,
//! so cells have nearly the same size everywhere (`cube.rs`).
//!
//! A tile at level `L` covers 1/2^L of a face along each axis with 64×64 cells, plus one
//! border sample on every side (taken from the neighbouring face where the tile touches a
//! face edge), so bicubic interpolation never needs another tile. Heights are
//! band-limited to the tile's cell size (`elevation.rs`). The interpolation is Catmull-Rom:
//! it passes through the samples and is C1 across cells and tiles, so surface normals (and
//! contact normals) turn smoothly.

use crate::cube::CubeCoord;
use crate::elevation::Elevation;
use creeer_math::glam::DVec3;
use rayon::prelude::*;
use std::cell::Cell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

thread_local! {
    /// Whether this thread generates tiles by itself (`generate_serially_here`).
    static SERIAL: Cell<bool> = const { Cell::new(false) };
}

/// Makes this thread generate height tiles by itself, not on the thread pool: a terrain
/// streamer's worker, given a core of its own (the pool's cores are the simulation's).
pub fn generate_serially_here() {
    SERIAL.with(|s| s.set(true));
}

/// Cells per tile edge.
pub const TILE_CELLS: usize = 64;
/// Samples per tile edge: cells + 1, plus one border sample on each side.
const SIDE: usize = TILE_CELLS + 3;
/// Deepest tile level (cells of about 3 cm on a 500 km planet).
pub const MAX_LEVEL: u8 = 19;
/// Largest angle per unit of face coordinate (rad): along a face's centre line,
/// dθ/ds = 1/1.2 (`cube.rs`); elsewhere it is smaller.
const MAX_ANGLE_PER_FACE_UNIT: f64 = 1.0 / 1.2;

/// Identifies a tile: face, quadtree level, and column/row on that level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileKey {
    /// Face 0..6.
    pub face: u8,
    /// Quadtree level (0 = the whole face).
    pub level: u8,
    /// Column, 0..2^level.
    pub i: u32,
    /// Row, 0..2^level.
    pub j: u32,
}

/// Size of a tile cell in face coordinates.
fn cell(level: u8) -> f64 {
    2.0 / (TILE_CELLS as f64 * f64::from(1u32 << level))
}

/// Largest distance between neighbouring samples of a level on a sphere of `radius` (m).
pub fn level_spacing(radius: f64, level: u8) -> f64 {
    radius * MAX_ANGLE_PER_FACE_UNIT * cell(level)
}

/// The finest height spacing (m): the generated ground has no detail finer than this, and
/// physics reads it there (`Terrain::density`).
pub const FINEST_SPACING: f64 = 1.0;

/// Coarsest level whose samples are at most `spacing` metres apart.
pub fn level_for_spacing(radius: f64, spacing: f64) -> u8 {
    (0..MAX_LEVEL)
        .find(|&level| level_spacing(radius, level) <= spacing)
        .unwrap_or(MAX_LEVEL)
}

impl TileKey {
    /// The tile of `level` containing a point.
    pub fn containing(c: CubeCoord, level: u8) -> Self {
        let n = 1u32 << level;
        let index = |s: f64| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped
            let i = ((s + 1.0) * 0.5 * f64::from(n)).floor().max(0.0) as u32;
            i.min(n - 1)
        };
        Self {
            face: c.face,
            level,
            i: index(c.s),
            j: index(c.t),
        }
    }

    /// Face coordinates of the tile's first (non-border) sample.
    fn origin(&self) -> (f64, f64) {
        let size = 2.0 / f64::from(1u32 << self.level);
        (
            -1.0 + f64::from(self.i) * size,
            -1.0 + f64::from(self.j) * size,
        )
    }
}

/// Heights of one tile.
#[derive(Clone, Debug)]
pub struct HeightTile {
    /// Which tile.
    pub key: TileKey,
    /// `SIDE × SIDE` heights (m), s fastest; index 0 is the border sample.
    heights: Vec<f32>,
    /// Lowest sample (m).
    pub min: f64,
    /// Highest sample (m).
    pub max: f64,
}

impl HeightTile {
    /// Samples the elevation for `key`.
    ///
    /// Its rows in parallel on the thread pool (each sample on its own: the same heights on
    /// any number of threads), unless this thread generates by itself
    /// ([`generate_serially_here`]): a tile is about 3 ms of noise, and a body reaching new
    /// ground generates it within its tick (eight avatars of a loaded save: 23 ms).
    pub fn generate(elevation: &Elevation, key: TileKey) -> Self {
        let (s0, t0) = key.origin();
        let step = cell(key.level);
        let spacing = level_spacing(elevation.radius, key.level);
        let row = |b: usize| -> Vec<f32> {
            (0..SIDE)
                .map(|a| {
                    let c = CubeCoord {
                        face: key.face,
                        s: s0 + (index(a) - 1.0) * step,
                        t: t0 + (index(b) - 1.0) * step,
                    };
                    #[allow(clippy::cast_possible_truncation)] // km fit f32 to < 1 mm
                    let h = elevation.height(c.direction(), spacing) as f32;
                    h
                })
                .collect()
        };
        let rows: Vec<Vec<f32>> = if SERIAL.with(Cell::get) {
            (0..SIDE).map(row).collect()
        } else {
            (0..SIDE).into_par_iter().map(row).collect()
        };
        let heights = rows.concat();
        let min = heights.iter().fold(f64::MAX, |m, h| m.min(f64::from(*h)));
        let max = heights.iter().fold(f64::MIN, |m, h| m.max(f64::from(*h)));
        Self {
            key,
            heights,
            min,
            max,
        }
    }

    /// Interpolated height (m) at face coordinates `(s, t)` inside the tile.
    pub fn sample(&self, s: f64, t: f64) -> f64 {
        let (s0, t0) = self.key.origin();
        let step = cell(self.key.level);
        let (i, fx) = split((s - s0) / step);
        let (j, fy) = split((t - t0) / step);
        let (wx, wy) = (catmull_rom(fx), catmull_rom(fy));
        let mut h = 0.0;
        for (dy, wy) in wy.iter().enumerate() {
            // Sample i−1 of the cell grid is index i of the stored grid (border offset).
            let row = &self.heights[(j + dy) * SIDE + i..][..4];
            let along: f64 = row.iter().zip(&wx).map(|(&v, w)| f64::from(v) * w).sum();
            h += along * wy;
        }
        h
    }

    /// Interpolated height (m) at face coordinates `(s, t)`, with its derivatives along `s`
    /// and `t` (m per face unit): the exact slopes of the surface `sample` describes.
    pub fn sample_with_slopes(&self, s: f64, t: f64) -> (f64, f64, f64) {
        let (s0, t0) = self.key.origin();
        let step = cell(self.key.level);
        let (i, fx) = split((s - s0) / step);
        let (j, fy) = split((t - t0) / step);
        let (wx, wy) = (catmull_rom(fx), catmull_rom(fy));
        let (dx, dy) = (catmull_rom_slope(fx), catmull_rom_slope(fy));
        let (mut h, mut hs, mut ht) = (0.0, 0.0, 0.0);
        for k in 0..4 {
            let row = &self.heights[(j + k) * SIDE + i..][..4];
            let along: f64 = row.iter().zip(&wx).map(|(&v, w)| f64::from(v) * w).sum();
            let slope: f64 = row.iter().zip(&dx).map(|(&v, w)| f64::from(v) * w).sum();
            h += along * wy[k];
            hs += slope * wy[k];
            ht += along * dy[k];
        }
        (h, hs / step, ht / step)
    }
}

fn index(i: usize) -> f64 {
    f64::from(u32::try_from(i).expect("small index"))
}

/// Cell index (clamped so the 4×4 stencil stays inside the tile) and fraction within it.
fn split(x: f64) -> (usize, f64) {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped
    let i = (x.floor().max(0.0) as usize).min(TILE_CELLS - 1);
    (i, x - index(i))
}

/// Catmull-Rom weights of the samples at −1, 0, 1, 2 for a point at `t` in [0, 1].
fn catmull_rom(t: f64) -> [f64; 4] {
    let (t2, t3) = (t * t, t * t * t);
    [
        0.5 * (-t3 + 2.0 * t2 - t),
        0.5 * (3.0 * t3 - 5.0 * t2 + 2.0),
        0.5 * (-3.0 * t3 + 4.0 * t2 + t),
        0.5 * (t3 - t2),
    ]
}

/// Derivatives of the Catmull-Rom weights with respect to `t`.
fn catmull_rom_slope(t: f64) -> [f64; 4] {
    let t2 = t * t;
    [
        0.5 * (-3.0 * t2 + 4.0 * t - 1.0),
        0.5 * (9.0 * t2 - 10.0 * t),
        0.5 * (-9.0 * t2 + 8.0 * t + 1.0),
        0.5 * (3.0 * t2 - 2.0 * t),
    ]
}

type TileMap = HashMap<TileKey, (Arc<HeightTile>, u64)>;

/// Generated tiles, shared between threads; the least recently used are dropped beyond
/// the capacity.
#[derive(Debug)]
pub struct HeightCache {
    elevation: Arc<Elevation>,
    capacity: usize,
    /// Tiles with the clock value of their last use, and the clock.
    tiles: Mutex<(TileMap, u64)>,
}

impl HeightCache {
    /// A cache holding up to `capacity` tiles (each about 18 KB).
    pub fn new(elevation: Arc<Elevation>, capacity: usize) -> Self {
        Self {
            elevation,
            capacity: capacity.max(16),
            tiles: Mutex::new((HashMap::new(), 0)),
        }
    }

    /// The elevation model.
    pub fn elevation(&self) -> &Elevation {
        &self.elevation
    }

    /// Number of cached tiles.
    pub fn len(&self) -> usize {
        self.lock().0.len()
    }

    /// True if no tile is cached.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, (TileMap, u64)> {
        self.tiles
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The tile for `key`, generated if needed (outside the lock, so threads generating
    /// different tiles do not wait for each other).
    pub fn tile(&self, key: TileKey) -> Arc<HeightTile> {
        {
            let mut guard = self.lock();
            let (map, clock) = &mut *guard;
            *clock += 1;
            if let Some((tile, used)) = map.get_mut(&key) {
                *used = *clock;
                return Arc::clone(tile);
            }
        }
        let tile = Arc::new(HeightTile::generate(&self.elevation, key));
        let mut guard = self.lock();
        let (map, clock) = &mut *guard;
        *clock += 1;
        let tile = Arc::clone(&map.entry(key).or_insert((tile, *clock)).0);
        if map.len() > self.capacity {
            // Drop the oldest quarter at once, so eviction is rare.
            let mut ages: Vec<u64> = map.values().map(|&(_, used)| used).collect();
            ages.sort_unstable();
            let cutoff = ages[map.len() / 4];
            map.retain(|_, (_, used)| *used > cutoff);
        }
        tile
    }

    /// A sampler at the level matching `spacing`, for many lookups in one area; never finer
    /// than [`FINEST_SPACING`] (G2.9: chunks of 12.5 cm cells draw the ground physics touches,
    /// and only edits are finer).
    pub fn sampler(&self, spacing: f64) -> HeightSampler<'_> {
        HeightSampler {
            cache: self,
            level: level_for_spacing(self.elevation.radius, spacing.max(FINEST_SPACING)),
            recent: Vec::new(),
        }
    }
}

/// Tiles a sampler keeps (a chunk touches at most a few).
const RECENT_TILES: usize = 16;

/// Looks up heights at one level, keeping the tiles it used (no locking after the first
/// lookup in a tile).
#[derive(Debug)]
pub struct HeightSampler<'a> {
    cache: &'a HeightCache,
    /// Tile level used.
    pub level: u8,
    recent: Vec<Arc<HeightTile>>,
}

impl HeightSampler<'_> {
    /// The tile containing direction `dir` (not necessarily unit), and its coordinates.
    pub fn tile(&mut self, dir: DVec3) -> (&HeightTile, CubeCoord) {
        let c = CubeCoord::from_direction(dir);
        let key = TileKey::containing(c, self.level);
        let at = match self.recent.iter().position(|t| t.key == key) {
            Some(at) => at,
            None => {
                if self.recent.len() == RECENT_TILES {
                    self.recent.remove(0);
                }
                self.recent.push(self.cache.tile(key));
                self.recent.len() - 1
            }
        };
        (&self.recent[at], c)
    }

    /// Height (m) above the datum in direction `dir`.
    pub fn height(&mut self, dir: DVec3) -> f64 {
        let (tile, c) = self.tile(dir);
        tile.sample(c.s, c.t)
    }

    /// Height (m) above the datum in the direction of point `p`, and its gradient with
    /// respect to `p` (per metre): exact for the interpolated surface.
    pub fn height_gradient(&mut self, p: DVec3) -> (f64, DVec3) {
        let (tile, c) = self.tile(p);
        let (h, hs, ht) = tile.sample_with_slopes(c.s, c.t);
        let (gs, gt) = c.gradients(p);
        (h, gs * hs + gt * ht)
    }
}
