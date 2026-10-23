//! The volume a dig takes out of the ground (G1.5, ADR-0032), measured from the same density
//! field the physics reads: the generated ground with every edit, rounded as the finest grid
//! rounds it. What a player digs is handed out as it is dug; the terrain is cut once enough
//! is dug for the smallest pit the grid draws, by a brush sized so the field loses exactly that.
//!
//! The field is sampled once on a grid of `step` about a centre (a column's generated density
//! is linear in height: one height lookup a column), and then a brush's removal is a sum over
//! the samples in rock: each one's share of rock, `clamp(½ − d/step)`, before and after.

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
        let up = centre.normalize();
        let (east, north) = tangent_frame(up);
        #[allow(clippy::cast_possible_truncation)] // a few dozen
        let n = (reach / step).ceil() as i32;
        let edits = terrain.edits();
        let mut samples = Vec::new();
        for i in -n..=n {
            for j in -n..=n {
                let column = centre + east * (f64::from(i) * step) + north * (f64::from(j) * step);
                let generated = terrain.generated_density(column);
                for k in -n..=n {
                    let z = f64::from(k) * step;
                    let p = column + up * z;
                    if (p - centre).length() > reach {
                        continue;
                    }
                    let d = edits.density(p, generated + z);
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
        let dig = Edit::Dig(*brush);
        let k = round(FINEST_CELL);
        let lost: f64 = (self.samples.iter())
            .map(|&(p, d)| rock(d, self.step) - rock(dig.apply(p, d, k), self.step))
            .sum();
        lost * self.step * self.step * self.step
    }

    /// The radius in `least..=most` of a sphere about `centre` that takes at most `volume` out
    /// of the field, within a thousandth of it, and what it takes (`most`'s if even that takes
    /// less); none if even `least`'s takes more. What is left over stays dug and not cut.
    pub fn sphere_for(
        &self,
        centre: DVec3,
        volume: f64,
        (least, most): (f64, f64),
    ) -> Option<(f64, f64)> {
        let take = |radius: f64| {
            self.removed(&Brush::Sphere {
                center: centre,
                radius,
            })
        };
        let bottom = take(least);
        if bottom > volume {
            return None;
        }
        let top = take(most);
        if top <= volume {
            return Some((most, top));
        }
        let (mut lo, mut hi, mut best) = (least, most, (least, bottom));
        for _ in 0..30 {
            if volume - best.1 <= 1e-3 * volume {
                break;
            }
            let mid = 0.5 * (lo + hi);
            let v = take(mid);
            if v <= volume {
                (lo, best) = (mid, (mid, v));
            } else {
                hi = mid;
            }
        }
        Some(best)
    }
}

#[cfg(test)]
#[path = "dug_tests.rs"]
mod tests;
