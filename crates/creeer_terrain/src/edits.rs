//! Terrain edits: digging and filling, stored as deltas against the seeded generator.
//!
//! An edit is a brush (sphere or capsule) that either digs (air inside the brush) or fills
//! (a chosen rock inside). Edits are kept as an ordered log and applied on top of the
//! generated density with CSG: dig `d = max(d, −b)`, fill `d = min(d, b)`, where `b` is the
//! brush's signed distance (negative inside). This works at every level of detail, needs a
//! few dozen bytes per edit, and later edits override earlier ones (ADR-0006).
//!
//! The max and min are smooth (G1.3c, ADR-0006 amended): where an edit meets the ground, a
//! dug rim is rounded off and a fill meets the ground in a fillet, over [`ROUND_CELLS`] cells
//! of the grid that draws it. A crease between them, sharp, came out of the one-vertex-a-cell
//! mesher as a zigzag of cell-sized facets (`docs/art-style.md` pillar 9). Physics reads the
//! finest grid's field, so what is drawn up close is what is touched.
//!
//! A brush smaller than half a cell cannot be resolved by a chunk and would alias into a
//! cell-sized bubble, so coarse chunks leave such brushes out ([`Edits::affecting`]).

use creeer_math::glam::{DVec3, IVec3};
use std::collections::HashMap;

/// The shape of an edit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Brush {
    /// A ball.
    Sphere {
        /// Centre (m, planet-centred).
        center: DVec3,
        /// Radius (m).
        radius: f64,
    },
    /// A segment with a radius (tunnels).
    Capsule {
        /// One end of the axis (m).
        a: DVec3,
        /// The other end (m).
        b: DVec3,
        /// Radius (m).
        radius: f64,
    },
}

impl Brush {
    /// Signed distance from `p` to the brush surface (negative inside).
    pub fn distance(&self, p: DVec3) -> f64 {
        match *self {
            Self::Sphere { center, radius } => (p - center).length() - radius,
            Self::Capsule { a, b, radius } => {
                let ab = b - a;
                let t = ((p - a).dot(ab) / ab.length_squared().max(1e-30)).clamp(0.0, 1.0);
                (p - (a + ab * t)).length() - radius
            }
        }
    }

    /// Gradient of [`Brush::distance`] at `p` (unit length; away from the axis).
    pub fn gradient(&self, p: DVec3) -> DVec3 {
        let axis = match *self {
            Self::Sphere { center, .. } => center,
            Self::Capsule { a, b, .. } => {
                let ab = b - a;
                let t = ((p - a).dot(ab) / ab.length_squared().max(1e-30)).clamp(0.0, 1.0);
                a + ab * t
            }
        };
        (p - axis).normalize_or(DVec3::Y)
    }

    /// Radius of the brush (m).
    pub fn radius(&self) -> f64 {
        match *self {
            Self::Sphere { radius, .. } | Self::Capsule { radius, .. } => radius,
        }
    }

    /// Axis-aligned bounds (m).
    pub fn bounds(&self) -> (DVec3, DVec3) {
        match *self {
            Self::Sphere { center, radius } => (center - radius, center + radius),
            Self::Capsule { a, b, radius } => (a.min(b) - radius, a.max(b) + radius),
        }
    }

    /// Volume (m³).
    pub fn volume(&self) -> f64 {
        let r = self.radius();
        let ball = 4.0 / 3.0 * std::f64::consts::PI * r * r * r;
        match *self {
            Self::Sphere { .. } => ball,
            Self::Capsule { a, b, .. } => ball + std::f64::consts::PI * r * r * (b - a).length(),
        }
    }
}

/// One edit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Edit {
    /// Remove all material inside the brush.
    Dig(Brush),
    /// Fill the brush with a rock (material id).
    Fill(Brush, u16),
}

impl Edit {
    /// Its brush.
    pub fn brush(&self) -> &Brush {
        match self {
            Self::Dig(b) | Self::Fill(b, _) => b,
        }
    }

    /// Applies the edit to a density value at `p`, rounded over `round` metres where it meets
    /// the surface ([`round`]); a brush smaller than the finest grid's round is rounded over
    /// its own radius at most (G1.5: a hand-dug pit is not rounded wider than itself).
    pub fn apply(&self, p: DVec3, density: f64, round: f64) -> f64 {
        let r = self.brush().radius();
        let round = if r < ROUND_CELLS * FINEST_CELL {
            round.min(r)
        } else {
            round
        };
        match self {
            Self::Dig(b) => smooth_max(density, -b.distance(p), round),
            Self::Fill(b, _) => smooth_min(density, b.distance(p), round),
        }
    }
}

/// The rounds where edits meet the ground span this many cells of the grid that draws them: a
/// round narrower than about two cells shows the cells' zigzag.
pub const ROUND_CELLS: f64 = 2.0;
/// The finest grid's cell (m): the field physics reads ([`Edits::density`]).
pub const FINEST_CELL: f64 = 1.0;

/// The width (m) of the rounds where edits meet the ground, on a grid of `cell` metres.
pub fn round(cell: f64) -> f64 {
    ROUND_CELLS * cell
}

/// `min(a, b)`, rounded where the two are within `k` of each other: at most `k/4` below it,
/// equal to it elsewhere (Quilez's polynomial smooth minimum).
pub fn smooth_min(a: f64, b: f64, k: f64) -> f64 {
    if k <= 0.0 {
        return a.min(b);
    }
    let h = (k - (a - b).abs()).max(0.0) / k;
    a.min(b) - 0.25 * k * h * h
}

/// `max(a, b)`, rounded as [`smooth_min`].
pub fn smooth_max(a: f64, b: f64, k: f64) -> f64 {
    -smooth_min(-a, -b, k)
}

/// Edge of the cells of the spatial index (m).
const INDEX_CELL: f64 = 16.0;
/// How far around its brush an edit is indexed (m): its round on the finest grid, and beyond
/// that rock just outside a dug brush has a density set by the brush (its distance to the dug
/// wall), not by the generator, and contact queries there must see the edit; penetrations
/// deeper than 2 m do not occur.
const INDEX_MARGIN: f64 = ROUND_CELLS * FINEST_CELL + 2.0;

/// The ordered log of edits, with a spatial index for point queries.
#[derive(Clone, Debug, Default)]
pub struct Edits {
    log: Vec<Edit>,
    index: HashMap<IVec3, Vec<u32>>,
}

fn index_cell(p: DVec3) -> IVec3 {
    #[allow(clippy::cast_possible_truncation)] // planet-sized coordinates fit i32 cells
    let cell = (p / INDEX_CELL).floor().as_ivec3();
    cell
}

impl Edits {
    /// Number of edits.
    pub fn len(&self) -> usize {
        self.log.len()
    }

    /// True if nothing was edited.
    pub fn is_empty(&self) -> bool {
        self.log.is_empty()
    }

    /// All edits in order.
    pub fn log(&self) -> &[Edit] {
        &self.log
    }

    /// Appends an edit.
    pub fn push(&mut self, edit: Edit) {
        let id = u32::try_from(self.log.len()).expect("fewer than 4 billion edits");
        let (lo, hi) = edit.brush().bounds();
        let (lo, hi) = (index_cell(lo - INDEX_MARGIN), index_cell(hi + INDEX_MARGIN));
        for z in lo.z..=hi.z {
            for y in lo.y..=hi.y {
                for x in lo.x..=hi.x {
                    self.index.entry(IVec3::new(x, y, z)).or_default().push(id);
                }
            }
        }
        self.log.push(edit);
    }

    /// The edits whose brush, widened by its round, overlaps the box `lo..hi` and that a grid
    /// of `cell` metres can resolve (radius at least half a cell), in order.
    pub fn affecting(&self, lo: DVec3, hi: DVec3, cell: f64) -> Vec<Edit> {
        let reach = round(cell);
        self.log
            .iter()
            .filter(|e| {
                let (blo, bhi) = e.brush().bounds();
                e.brush().radius() >= 0.5 * cell
                    && (blo - reach).cmple(hi).all()
                    && (bhi + reach).cmpge(lo).all()
            })
            .copied()
            .collect()
    }

    /// The edits whose brush may contain `p`, in order.
    pub fn near(&self, p: DVec3) -> impl Iterator<Item = &Edit> {
        self.index
            .get(&index_cell(p))
            .into_iter()
            .flatten()
            .map(|&i| &self.log[i as usize])
    }

    /// Applies every edit at `p` to a generated density, with the finest grid's rounds.
    pub fn density(&self, p: DVec3, generated: f64) -> f64 {
        let round = round(FINEST_CELL);
        self.near(p).fold(generated, |d, e| e.apply(p, d, round))
    }

    /// The rock of the last fill among `edits` whose brush contains `p` (within `tolerance`
    /// metres of its surface), unless a later dig took that place; `None` means the
    /// generator's rock.
    pub fn material(edits: &[Edit], p: DVec3, tolerance: f64) -> Option<u16> {
        edits.iter().rev().find_map(|e| {
            (e.brush().distance(p) <= tolerance).then_some(match e {
                Edit::Fill(_, rock) => Some(*rock),
                Edit::Dig(_) => None,
            })
        })?
    }
}

/// File signature of a saved edit log.
const MAGIC: &[u8; 8] = b"CREEDITS";
/// Format version of [`Edits::to_bytes`].
pub const FORMAT_VERSION: u32 = 1;

/// Why a saved edit log could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditsError(pub String);

impl std::fmt::Display for EditsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for EditsError {}

impl Edits {
    /// The log as bytes: signature, version, count, then each edit (little-endian; floats
    /// bit-exact, so a loaded world is identical to the saved one).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(20 + self.log.len() * 59);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        out.extend_from_slice(&(self.log.len() as u64).to_le_bytes());
        for edit in &self.log {
            let (fill, rock) = match edit {
                Edit::Dig(_) => (0u8, 0u16),
                Edit::Fill(_, rock) => (2, *rock),
            };
            let floats: Vec<f64> = match *edit.brush() {
                Brush::Sphere { center, radius } => {
                    out.push(fill);
                    vec![center.x, center.y, center.z, radius]
                }
                Brush::Capsule { a, b, radius } => {
                    out.push(fill + 1);
                    vec![a.x, a.y, a.z, b.x, b.y, b.z, radius]
                }
            };
            out.extend_from_slice(&rock.to_le_bytes());
            for f in floats {
                out.extend_from_slice(&f.to_le_bytes());
            }
        }
        out
    }

    /// Reads a log written by [`Self::to_bytes`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, EditsError> {
        let err = |what: &str| EditsError(format!("edit log: {what}"));
        let mut at = 0usize;
        let mut take = |n: usize| -> Result<&[u8], EditsError> {
            let slice = bytes.get(at..at + n).ok_or_else(|| err("truncated"))?;
            at += n;
            Ok(slice)
        };
        if take(8)? != MAGIC {
            return Err(err("not an edit log"));
        }
        let version = u32::from_le_bytes(take(4)?.try_into().expect("4 bytes"));
        if version != FORMAT_VERSION {
            return Err(err(&format!("unsupported version {version}")));
        }
        let count = u64::from_le_bytes(take(8)?.try_into().expect("8 bytes"));
        let mut edits = Self::default();
        for _ in 0..count {
            let tag = take(1)?[0];
            let rock = u16::from_le_bytes(take(2)?.try_into().expect("2 bytes"));
            let mut float = || -> Result<f64, EditsError> {
                let f = f64::from_le_bytes(take(8)?.try_into().expect("8 bytes"));
                if f.is_finite() {
                    Ok(f)
                } else {
                    Err(err("non-finite number"))
                }
            };
            let brush = match tag & 1 {
                0 => Brush::Sphere {
                    center: DVec3::new(float()?, float()?, float()?),
                    radius: float()?,
                },
                _ => Brush::Capsule {
                    a: DVec3::new(float()?, float()?, float()?),
                    b: DVec3::new(float()?, float()?, float()?),
                    radius: float()?,
                },
            };
            if brush.radius() <= 0.0 {
                return Err(err("brush radius must be positive"));
            }
            edits.push(match tag {
                0 | 1 => Edit::Dig(brush),
                2 | 3 => Edit::Fill(brush, rock),
                _ => return Err(err(&format!("unknown edit kind {tag}"))),
            });
        }
        if at != bytes.len() {
            return Err(err("trailing bytes"));
        }
        Ok(edits)
    }
}
