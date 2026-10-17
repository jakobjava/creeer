//! A level of detail's own surface, and the next coarser one, at every vertex: what the
//! renderer needs to blend one level into the next without seams (geomorphing, after
//! Strugar's CDLOD, 2010).
//!
//! A chunk with cells of size c sees a density field band-limited to c: heights from tiles
//! of that spacing, and only the edits it can resolve (brushes of at least c/2). Its
//! neighbour one level coarser sees the field band-limited to 2c. Where the two meet, both
//! shapes and shading differ — the faint lines at level borders (measured: they vanish
//! when every vertex gets the same normal, ADR-0006/0007). So every vertex carries:
//!
//! - its position projected onto its own level's surface, and that surface's normal,
//!   computed from the same function by every chunk of the level (neighbours agree);
//! - the next coarser level's surface: how far to move along the normal to reach it, and
//!   its normal there.
//!
//! The renderer moves vertices and turns normals towards the coarser level over a band of
//! distance that ends where a coarser neighbour can begin (`stream.rs`): there a chunk looks
//! exactly like the coarser level.

use crate::datum::Datum;
use crate::edits::Edit;
use crate::terrain::{Terrain, generated};
use crate::tiles::HeightSampler;
use creeer_math::glam::{DVec3, Vec3};

/// The density field one level of detail sees around a chunk.
pub(crate) struct LevelField<'a> {
    sampler: HeightSampler<'a>,
    datum: Datum,
    edits: Vec<Edit>,
    /// Cell size of the level (m): the band limit and the gradient step.
    cell: f64,
}

impl<'a> LevelField<'a> {
    /// The field of the level with cells of `cell` metres, for points between `lo` and `hi`.
    pub(crate) fn new(terrain: &'a Terrain, lo: DVec3, hi: DVec3, cell: f64) -> Self {
        Self {
            sampler: terrain.heights().sampler(cell),
            datum: terrain.planet().datum,
            edits: terrain.edits().affecting(lo - cell, hi + cell, cell),
            cell,
        }
    }

    /// Density (m): the height above this level's surface; negative in rock.
    pub(crate) fn density(&mut self, p: DVec3) -> f64 {
        let generated = generated(&self.datum, &mut self.sampler, p);
        self.edits.iter().fold(generated, |d, e| e.apply(p, d))
    }

    /// The density at `p` and its gradient (points into the air).
    ///
    /// The generated part's gradient is exact: the derivative of the interpolated heights,
    /// the datum and the distance from the centre (one height lookup instead of six). Edits
    /// combine by max/min, which has creases: an edit whose surface stays at least `reach`
    /// from the generated one around `p` either has no part in the value or is all of it,
    /// and its gradient is the brush's. Where it comes closer, the crease may cross the
    /// stencil, and the gradient is central differences over one cell, as everywhere before.
    pub(crate) fn value_gradient(&mut self, p: DVec3) -> (f64, DVec3) {
        let (density, gradient, _) = self.sample(p);
        (density, gradient)
    }

    /// As [`Self::value_gradient`], and whether an edit's crease may lie within a cell of `p`.
    pub(crate) fn sample(&mut self, p: DVec3) -> (f64, DVec3, bool) {
        if p == DVec3::ZERO {
            return (self.density(p), DVec3::ZERO, false); // the centre: no direction, no slope
        }
        let (height, height_gradient) = self.sampler.height_gradient(p);
        let (radius, radius_gradient) = self.datum.radius_gradient(p);
        let length = p.length();
        let (mut d, mut g) = (
            length - radius - height,
            p / length - radius_gradient - height_gradient,
        );
        for e in &self.edits {
            // Over one cell each way, a brush distance moves at most a cell, the density
            // about |g| cells; twice that for its curvature.
            let reach = 2.0 * self.cell * (1.0 + g.length());
            let (value, sign) = match e {
                Edit::Dig(b) => (-b.distance(p), -1.0),
                Edit::Fill(b, _) => (b.distance(p), 1.0),
            };
            // Dig keeps the larger value, fill the smaller: does the brush win?
            let margin = sign * (d - value);
            if margin > reach {
                (d, g) = (value, e.brush().gradient(p) * sign);
            } else if margin > -reach {
                return (self.density(p), self.numeric_gradient(p), true);
            }
        }
        (d, g, false)
    }

    /// Gradient of the density by central differences over one cell.
    pub(crate) fn numeric_gradient(&mut self, p: DVec3) -> DVec3 {
        let h = self.cell;
        DVec3::new(
            self.density(p + DVec3::X * h) - self.density(p - DVec3::X * h),
            self.density(p + DVec3::Y * h) - self.density(p - DVec3::Y * h),
            self.density(p + DVec3::Z * h) - self.density(p - DVec3::Z * h),
        ) / (2.0 * h)
    }

    /// The signed distance t along unit `dir` from `p` (where the density is `start`) to
    /// this level's surface (density 0), by Newton steps with the slope `slope` = gradient·dir
    /// at `p`, at most `limit` either way.
    pub(crate) fn along(
        &mut self,
        p: DVec3,
        dir: DVec3,
        (start, slope): (f64, f64),
        limit: f64,
    ) -> f64 {
        if slope.abs() < 1e-3 {
            return 0.0; // the surface runs along dir here: no crossing to find
        }
        let mut t = 0.0;
        for k in 0..3 {
            let density = if k == 0 {
                start
            } else {
                self.density(p + dir * t)
            };
            let step = -density / slope;
            t = (t + step).clamp(-limit, limit);
            if step.abs() < 1e-3 * self.cell {
                break;
            }
        }
        t
    }
}

/// A vertex as the renderer gets it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Refined {
    /// Position (world, m): on its own level's surface, or a cap's corner.
    pub position: DVec3,
    /// That surface's normal.
    pub normal: Vec3,
    /// The next coarser level's normal at its surface.
    pub coarse_normal: Vec3,
    /// Distance along `normal` to the next coarser level's surface (m).
    pub morph: f32,
    /// The point of the surface the vertex stands for (its position, or for a cap vertex
    /// the surface along its normal): rock and openness are taken there.
    pub anchor: DVec3,
}

/// Both fields a chunk with cells of `cell` metres needs.
pub(crate) struct Levels<'a> {
    own: LevelField<'a>,
    coarse: LevelField<'a>,
}

impl<'a> Levels<'a> {
    /// The fields around the chunk from `lo` to `hi` with cells of `cell` metres.
    pub(crate) fn new(terrain: &'a Terrain, lo: DVec3, hi: DVec3, cell: f64) -> Self {
        Self {
            own: LevelField::new(terrain, lo, hi, cell),
            coarse: LevelField::new(terrain, lo, hi, 2.0 * cell),
        }
    }

    /// Refines a Surface Nets vertex at `p` of the cell from `low` with edge `cell` (with the
    /// normal of its cell, `cell_normal`, as a fallback where the field is flat): onto its
    /// level's surface, with the coarser level. It stays within its cell, so the cells keep
    /// their order and chunks can clip their meshes cell by cell (`clip.rs`).
    ///
    /// Where rock or air is thinner than a cell (a tunnel's roof), two surfaces lie within
    /// reach and the search along the normal may find the far one, facing the other way: the
    /// vertex then stays where Surface Nets put it (measured in F5: flaps across openings).
    /// So does a vertex near an edit's crease (a tunnel's rim): Surface Nets keeps it near the
    /// edge, while moving it along a normal averaged over both sides would cut the edge off.
    pub(crate) fn refine(
        &mut self,
        p: DVec3,
        cell_normal: Vec3,
        (low, cell): (DVec3, f64),
    ) -> Refined {
        let (density, gradient, crease) = self.own.sample(p);
        let start = gradient.try_normalize().unwrap_or(cell_normal.as_dvec3());
        if crease {
            let (coarse_normal, morph) = self.coarser(p, start);
            return Refined {
                position: p,
                normal: start.as_vec3(),
                coarse_normal,
                morph,
                anchor: p,
            };
        }
        // A Surface Nets vertex lies inside the cell the surface crosses: within a cell of it.
        let t = self
            .own
            .along(p, start, (density, gradient.dot(start)), cell);
        let found = (p + start * t).clamp(low, low + DVec3::splat(cell));
        // The normal where the vertex ends up: the surface's own there.
        let (position, normal) = match self.own.value_gradient(found).1.try_normalize() {
            Some(normal) if normal.dot(start) > 0.0 => (found, normal),
            _ => (p, start),
        };
        let (coarse_normal, morph) = self.coarser(position, normal);
        Refined {
            position,
            normal: normal.as_vec3(),
            coarse_normal,
            morph,
            anchor: position,
        }
    }

    /// A cap vertex at `p` (in rock, or on the surface; `depth` below it as the samples
    /// estimate): it stays there, and moves with the surface along its normal, as the surface
    /// point it stands for does. Morphing moves the surface at most two coarse cells, so a
    /// corner deeper than three stays below it without moving, and needs no field lookups.
    pub(crate) fn fixed(&mut self, p: DVec3, sample_normal: Vec3, depth: f64) -> Refined {
        let cell = self.own.cell;
        if depth > 3.0 * self.coarse.cell {
            return Refined {
                position: p,
                normal: sample_normal,
                coarse_normal: sample_normal,
                morph: 0.0,
                anchor: p + sample_normal.as_dvec3() * depth,
            };
        }
        let (density, gradient) = self.own.value_gradient(p);
        let normal = gradient.try_normalize().unwrap_or(sample_normal.as_dvec3());
        // Cap corners in rock next to the border lie within a cell or two of the surface.
        let slope = (density, gradient.dot(normal));
        let anchor = p + normal * self.own.along(p, normal, slope, 4.0 * cell);
        let (coarse_normal, morph) = self.coarser(anchor, normal);
        Refined {
            position: p,
            normal: normal.as_vec3(),
            coarse_normal,
            morph,
            anchor,
        }
    }

    /// The coarser level's normal, and the distance to it along `normal`, from `position`.
    fn coarser(&mut self, position: DVec3, normal: DVec3) -> (Vec3, f32) {
        let (density, gradient) = self.coarse.value_gradient(position);
        let slope = (density, gradient.dot(normal));
        let morph = self
            .coarse
            .along(position, normal, slope, 2.0 * self.coarse.cell);
        let at = position + normal * morph;
        let coarse_normal = self
            .coarse
            .value_gradient(at)
            .1
            .try_normalize()
            .unwrap_or(normal);
        #[allow(clippy::cast_possible_truncation)] // a few cells: fits f32
        (coarse_normal.as_vec3(), morph as f32)
    }
}
