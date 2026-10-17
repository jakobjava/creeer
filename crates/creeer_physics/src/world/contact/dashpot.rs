//! The contacts' dashpots (`mod.rs`): each damps its own penetration rate, from its gap's
//! reference at the prediction.
//!
//! A point damps while it is pressed in: c/(2β)·(min(g, 0) − min(g_r, 0))². A face's dashpot
//! is spread over the face as its pressure is: the points of the face that press at the
//! step's start (the midpoints of its triangles' edges, each with a third of its triangle's
//! area) each damp their own penetration rate for the step. A face pressed over a part f of
//! its area is as stiff as f of it, and damped critically for that, √f of the whole face's
//! damping: the pressed part gets the face's damping per area over √f.
//!
//! (Damped at its corners while pressed in, a lifted corner's force went undamped, and a
//! rock on bumpy ground jittered at a centimetre a second for ten seconds. Shared by area
//! alone, a crate on rough terrain pressing on part of its face was damped by f; clamped at
//! every evaluation, the points at the edge of the pressed part switched on and off within
//! the step; either way it never came to rest. Damping the face as stiff as it is, unclamped
//! (C = τ·K), held faces lifting off within a step, and turned a landing cube's spin back,
//! F5.)

use super::{Contacts, Law};

/// A face's dashpot for the step: per point of it, its coefficient (zero where it does not
/// press), and on a plane the same as a quadratic form in its corners' rates, Σ c·w·wᵀ (each
/// point's rate the corners' blend there): one product instead of a sum over the points.
#[derive(Clone, Debug, Default)]
pub(super) struct FaceDashpot {
    coefficients: Vec<f64>,
    form: [[f64; 4]; 4],
}

impl Contacts {
    /// Sets each face's dashpot for the step, `gaps` its contacts' gaps at the step's start.
    pub(super) fn start_dashpots(&mut self, gaps: &[f64], measured: &[Vec<f64>]) {
        self.face_dashpots = self
            .patches
            .iter()
            .zip(measured)
            .map(|(p, measured)| {
                let depths = std::array::from_fn(|k| -gaps[p.first + k]);
                let at = p.triangle_depths(depths, measured);
                let total: f64 = self.list[p.first..p.first + 4]
                    .iter()
                    .map(|c| c.damping)
                    .sum();
                let area: f64 = p.dashpot.iter().map(|q| q.0).sum();
                let pressed: Vec<bool> = p
                    .dashpot
                    .iter()
                    .map(|(_, _, [t, j])| at[*t][*j] + at[*t][(j + 1) % 3] > 0.0)
                    .collect();
                let part: f64 = p
                    .dashpot
                    .iter()
                    .zip(&pressed)
                    .filter(|q| *q.1)
                    .map(|q| q.0.0)
                    .sum();
                let per_area = if part > 0.0 {
                    total / area / (part / area).sqrt()
                } else {
                    0.0
                };
                let coefficients: Vec<f64> = p
                    .dashpot
                    .iter()
                    .zip(&pressed)
                    .map(|(q, &on)| if on { per_area * q.0 } else { 0.0 })
                    .collect();
                let mut form = [[0.0; 4]; 4];
                for (&(_, w, _), &c) in p.dashpot.iter().zip(&coefficients) {
                    for (row, wk) in form.iter_mut().zip(w) {
                        for (f, wl) in row.iter_mut().zip(w) {
                            *f += c * wk * wl;
                        }
                    }
                }
                FaceDashpot { coefficients, form }
            })
            .collect();
    }

    /// The dashpots' energy at `gaps` (and the depths `measured` at the patches' points over
    /// curved ground), its derivative by each contact's gap, and by each measured point's.
    pub(super) fn dashpots(
        &self,
        gaps: &[f64],
        measured: &[Vec<f64>],
    ) -> (f64, Vec<f64>, Vec<Vec<f64>>) {
        let reference = |i: usize| self.references.get(i).copied().unwrap_or(gaps[i]);
        let mut energy = 0.0;
        let mut slope = vec![0.0; gaps.len()];
        for (i, c) in self.list.iter().enumerate() {
            if matches!(c.law, Law::Hertz(_)) {
                let (now, then) = (gaps[i].min(0.0), reference(i).min(0.0));
                energy += c.damping / (2.0 * self.beta) * (now - then) * (now - then);
                if gaps[i] < 0.0 {
                    slope[i] = c.damping / self.beta * (now - then);
                }
            }
        }
        let mut at_points = Vec::with_capacity(self.patches.len());
        for (pi, (p, face)) in self.patches.iter().zip(&self.face_dashpots).enumerate() {
            // Over curved ground the measured points' own rates: the corners' gaps to the
            // quadric under the face move with the face's sliding too, far from where it is taken.
            let points = p.sampled().zip(measured.get(pi).filter(|m| !m.is_empty()));
            let mut point_slope = vec![0.0; points.map_or(0, |(s, _)| s.vertices.len())];
            if points.is_none() {
                // On a plane: the form in the corners' rates.
                let rate: [f64; 4] =
                    std::array::from_fn(|k| gaps[p.first + k] - reference(p.first + k));
                for (k, row) in face.form.iter().enumerate() {
                    let pushed: f64 = row.iter().zip(&rate).map(|(f, r)| f * r).sum();
                    energy += rate[k] * pushed / (2.0 * self.beta);
                    slope[p.first + k] += pushed / self.beta;
                }
                at_points.push(point_slope);
                continue;
            }
            for (&(_, _, [t, j]), &c) in p.dashpot.iter().zip(&face.coefficients) {
                if c == 0.0 {
                    continue;
                }
                // The point's penetration rate: its gap from its reference.
                if let Some((s, measured)) = points {
                    let ends = [s.triangles[t][j], s.triangles[t][(j + 1) % 3]];
                    let rate: f64 = ends
                        .iter()
                        .map(|&v| 0.5 * (-measured[v] - self.point_reference(pi, v, -measured[v])))
                        .sum();
                    energy += c / (2.0 * self.beta) * rate * rate;
                    for v in ends {
                        point_slope[v] += c / self.beta * rate * 0.5;
                    }
                }
            }
            at_points.push(point_slope);
        }
        (energy, slope, at_points)
    }

    /// The reference gap of patch `p`'s measured point `v` (`gap` if none is set).
    fn point_reference(&self, p: usize, v: usize, gap: f64) -> f64 {
        self.point_references
            .get(p)
            .and_then(|r| r.get(v))
            .copied()
            .unwrap_or(gap)
    }
}

impl FaceDashpot {
    /// Its form in the corners' rates (N·s/m): exact on a plane; over curved ground, where its
    /// points damp their own rates, their rates taken as the corners' blend.
    pub(super) fn form(&self) -> [[f64; 4]; 4] {
        self.form
    }
}
