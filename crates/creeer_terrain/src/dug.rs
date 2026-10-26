//! The volume a dig takes out of the ground (G1.5, ADR-0032), measured from the same density
//! field the physics reads: the generated ground with every edit, rounded as the finest grid
//! rounds it. Every blow's ground is cut at once (G2.9): a dig spot's one brush grows, sized so
//! the field loses exactly what was dug there, to a thousandth.
//!
//! The field is sampled once on a grid of `step` about a centre (a column's generated density
//! is linear in height: one height lookup a column), and then a brush's removal is a sum over
//! the samples in rock: each one's share of rock, `clamp(½ − d/step)`, before and after. A hole
//! grown is measured only where it reaches beyond what it was: elsewhere both take the same.

use crate::Terrain;
use crate::edits::{Brush, Edit, FINEST_CELL, round};
use crate::geology::deposits::tangent_frame;
use creeer_math::glam::DVec3;

/// The field sampled about a point.
#[derive(Clone, Debug)]
pub struct Field {
    step: f64,
    /// The samples that are rock or within a step of it: where and their density (m).
    samples: Vec<(DVec3, f64)>,
}

/// How much of a sample's cell is rock at density `d` on a grid of `step`.
fn rock(d: f64, step: f64) -> f64 {
    (0.5 - d / step).clamp(0.0, 1.0)
}

impl Field {
    /// `terrain`'s field within `reach` of `centre`, every `step` metres.
    pub fn around(terrain: &Terrain, centre: DVec3, reach: f64, step: f64) -> Self {
        let keep = |p: DVec3| (p - centre).length() <= reach;
        Self::sampled(terrain, centre, (DVec3::splat(reach), step), None, keep)
    }

    /// `terrain`'s field in a box about `centre` (its half sizes east, north and up, as
    /// `tangent_frame` turns them), every `step` metres, without edit `without` (a dig spot's
    /// own brush, to be grown).
    pub fn in_box_without(
        terrain: &Terrain,
        centre: DVec3,
        (half, step): (DVec3, f64),
        without: Option<usize>,
    ) -> Self {
        Self::sampled(terrain, centre, (half, step), without, |_| true)
    }

    /// As [`Self::in_box_without`], only where brush `to` reaches beyond `from` (each grown
    /// from the other: elsewhere both dig the same, and a blow measuring the whole hole took
    /// 2–20 ms as it deepened, G2.9).
    pub fn beyond_without(
        terrain: &Terrain,
        (from, to): (Brush, Brush),
        (centre, half, step): (DVec3, DVec3, f64),
        without: Option<usize>,
    ) -> Self {
        let margin = round(FINEST_CELL).min(to.radius()) + step;
        let keep = |p: DVec3| {
            let d = to.distance(p);
            d <= margin && d < from.distance(p) - 1e-12
        };
        Self::sampled(terrain, centre, (half, step), without, keep)
    }

    fn sampled(
        terrain: &Terrain,
        centre: DVec3,
        (half, step): (DVec3, f64),
        without: Option<usize>,
        keep: impl Fn(DVec3) -> bool,
    ) -> Self {
        let up = centre.normalize();
        let (east, north) = tangent_frame(up);
        #[allow(clippy::cast_possible_truncation)] // a few dozen
        let n = (half / step).ceil().as_ivec3();
        let reach = half.length();
        let k = round(FINEST_CELL);
        let edits = terrain.edits();
        let (lo, hi) = (centre - reach - k, centre + reach + k);
        let near: Vec<Edit> = (edits.log().iter().enumerate())
            .filter(|&(i, e)| {
                let (blo, bhi) = e.brush().bounds();
                Some(i) != without && blo.cmple(hi).all() && bhi.cmpge(lo).all()
            })
            .map(|(_, e)| *e)
            .collect();
        let mut samples = Vec::new();
        for i in -n.x..=n.x {
            for j in -n.y..=n.y {
                let column = centre + east * (f64::from(i) * step) + north * (f64::from(j) * step);
                let mut generated = None;
                for h in -n.z..=n.z {
                    let z = f64::from(h) * step;
                    let p = column + up * z;
                    if !keep(p) {
                        continue;
                    }
                    let g = *generated.get_or_insert_with(|| terrain.generated_density(column));
                    let d = near.iter().fold(g + z, |d, e| e.apply(p, d, k));
                    if d < step {
                        samples.push((p, d));
                    }
                }
            }
        }
        Self { step, samples }
    }

    /// The volume of rock in the field (m³).
    pub fn volume(&self) -> f64 {
        let rock: f64 = self.samples.iter().map(|&(_, d)| rock(d, self.step)).sum();
        rock * self.step * self.step * self.step
    }

    /// The volume (m³) a dig with `brush` would take out of the field.
    pub fn removed(&self, brush: &Brush) -> f64 {
        self.removed_beyond(None, brush)
    }

    /// The volume (m³) a dig with `brush` would take out of the field beyond what one with
    /// `from` would (none: all it takes).
    pub fn removed_beyond(&self, from: Option<&Brush>, brush: &Brush) -> f64 {
        let dig = Edit::Dig(*brush);
        let was = from.map(|b| Edit::Dig(*b));
        let k = round(FINEST_CELL);
        let (lo, hi) = brush.bounds();
        let (lo, hi) = (lo - k - self.step, hi + k + self.step);
        let lost: f64 = (self.samples.iter())
            .filter(|(p, _)| p.cmpge(lo).all() && p.cmple(hi).all())
            .map(|&(p, d)| {
                let before = was.map_or(d, |w| w.apply(p, d, k));
                rock(before, self.step) - rock(dig.apply(p, d, k), self.step)
            })
            .sum();
        lost * self.step * self.step * self.step
    }

    /// The parameter in `lo..=hi` at which `shape`'s brush (growing with it, each containing
    /// the ones before) takes `volume` out of the field beyond what `lo`'s does (with `beyond`;
    /// else all it takes), within a thousandth of it (regula falsi, Illinois), and what it
    /// takes; `hi`'s if even that takes less.
    pub fn grown_for(
        &self,
        volume: f64,
        (shape, beyond): (impl Fn(f64) -> Brush, bool),
        (lo, hi): (f64, f64),
    ) -> (f64, f64) {
        let from = beyond.then(|| shape(lo));
        let take = |t: f64| self.removed_beyond(from.as_ref(), &shape(t));
        let (mut a, mut b) = ((lo, take(lo) - volume), (hi, take(hi) - volume));
        if b.1 <= 0.0 {
            return (hi, b.1 + volume);
        }
        if a.1 >= 0.0 {
            return (lo, a.1 + volume);
        }
        let mut side = 0;
        for _ in 0..60 {
            let t = (a.0 * b.1 - b.0 * a.1) / (b.1 - a.1);
            let f = take(t) - volume;
            if f.abs() <= 1e-3 * volume || (b.0 - a.0) < 1e-9 {
                return (t, f + volume);
            }
            if f < 0.0 {
                a = (t, f);
                if side == -1 {
                    b.1 *= 0.5;
                }
                side = -1;
            } else {
                b = (t, f);
                if side == 1 {
                    a.1 *= 0.5;
                }
                side = 1;
            }
        }
        // (Never in a field of steps: the closest below.)
        (a.0, a.1 + volume)
    }
}

#[cfg(test)]
#[path = "dug_tests.rs"]
mod tests;
