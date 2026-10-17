//! A patch over curved ground (`patch.rs`, `Domain::Sampled`): the depths measured at its
//! triangles' corners, and the forces there.

use super::super::integrals::{pressed_area, square_integral};
use super::Patch;
use creeer_math::glam::DVec3;

/// The depth a face just touching curved ground is taken to press at (m; the cap's reach if
/// deeper, `Patch::punch_area`).
const PUNCH_DEPTH: f64 = 1e-6;
/// The least area a face presses as (m²): a millimetre square.
const LEAST_PUNCH: f64 = 1e-6;

impl Patch {
    /// The face's motion σ at corner depths `depths` predicted to first order from where it was
    /// `found` (σ at its depths there), with the slope dσ/dδ of the step's start
    /// (`Self::prepare`).
    pub fn predict(&self, found: (DVec3, [f64; 4]), depths: [f64; 4]) -> DVec3 {
        let slope = &self.start.2;
        (0..4).fold(found.0, |g, k| g + slope[k] * (depths[k] - found.1[k]))
    }

    /// As [`Self::evaluate_on`], its search for σ starting from `guess` (σ where the patch was
    /// last evaluated in the step: a line search's trials lie close together). σ is found to
    /// `DECREMENT`, so E is the same function of the depths from any start.
    pub fn evaluate_from(
        &self,
        depths: [f64; 4],
        measured: &[f64],
        guess: DVec3,
    ) -> (f64, [f64; 4], DVec3) {
        self.minimise(depths, &self.offsets(depths, measured), guess)
    }

    /// The force at each measured point (dE/dδ there, which is what moves the bodies; none on a
    /// plane), at corner depths `depths` and depths `measured` and the motion σ
    /// [`Self::evaluate_on`] found there.
    pub fn point_forces(&self, depths: [f64; 4], measured: &[f64], sigma: DVec3) -> Vec<f64> {
        self.at_points(depths, &self.offsets(depths, measured), sigma)
    }

    /// How far the depths `measured` at the triangles' corners lie off the face's corners'
    /// blend `depths`, per triangle and corner (none for a face on a plane).
    pub(super) fn offsets(&self, depths: [f64; 4], measured: &[f64]) -> Vec<[f64; 3]> {
        let Some(sampled) = self.sampled.as_ref().filter(|_| !measured.is_empty()) else {
            return Vec::new();
        };
        self.triangles
            .iter()
            .zip(&sampled.triangles)
            .map(|(&(_, w), corners)| {
                std::array::from_fn(|j| {
                    measured[corners[j]] - (0..4).map(|k| w[j][k] * depths[k]).sum::<f64>()
                })
            })
            .collect()
    }

    /// The depth at the corners of each triangle for corner depths `depths` and the depths
    /// `measured` at them (as [`Self::evaluate_on`]).
    pub fn triangle_depths(&self, depths: [f64; 4], measured: &[f64]) -> Vec<[f64; 3]> {
        let offsets = self.offsets(depths, measured);
        (0..self.triangles.len())
            .map(|t| self.depths_at(t, depths, &offsets))
            .collect()
    }

    /// The area a face over curved ground presses with as found (`at`, the depth at each
    /// triangle's corners), as a punch: where it is pressed in, or, pressed in less than the
    /// cap's reach (just touching), would be at that depth. A face pressing near a point of a hump is then as stiff as Hertz's contact
    /// there: for ground rising as r²/2R, A = 2π·R·δ and κ·∫δ = √2·E*·√R·δ^{3/2}, against Hertz's
    /// 4/3. (Pressing as the whole face's punch, a 2 m crate on a 15 cm bump sank 11 mm into
    /// it, a hundred times Hertz's depth, F5.) It is taken as the face is found and kept
    /// for the step, set with the cap before any energy is measured: the step's energy stays
    /// convex in the depths, and the energy guard measures the same law before and after.
    /// (Set as the step began, after the guard had measured its energy, a crate at rest on
    /// terrain gained energy every step, and its steps fell back to backward Euler, F5.)
    pub(super) fn punch_area(&self, at: &[[f64; 3]]) -> f64 {
        let deepest = at
            .iter()
            .flatten()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let reach = self.cap.map_or(PUNCH_DEPTH, |c| c.reach.max(PUNCH_DEPTH));
        let lift = (reach - deepest).max(0.0);
        let area: f64 = at
            .iter()
            .zip(&self.triangles)
            .map(|(f, &(area, _))| pressed_area(f.map(|d| d + lift), area))
            .sum();
        area.clamp(LEAST_PUNCH, self.own.max(LEAST_PUNCH))
    }

    /// The force at each measured point (none on a plane): the material's, at the minimising
    /// motion σ.
    pub(super) fn at_points(
        &self,
        depths: [f64; 4],
        offsets: &[[f64; 3]],
        sigma: DVec3,
    ) -> Vec<f64> {
        let Some(sampled) = self.sampled.as_ref().filter(|_| !offsets.is_empty()) else {
            return Vec::new();
        };
        let shifted: [f64; 4] =
            std::array::from_fn(|k| depths[k] - DVec3::from(self.rigid[k]).dot(sigma));
        let mut forces = vec![0.0; sampled.vertices.len()];
        for (t, corners) in sampled.triangles.iter().enumerate() {
            let f = self.depths_at(t, shifted, offsets);
            if f.iter().any(|&d| d > 0.0) {
                let (_, gradient) = square_integral(f, self.triangles[t].0);
                for j in 0..3 {
                    forces[corners[j]] += 0.5 * self.modulus * gradient[j];
                }
            }
        }
        forces
    }
}
