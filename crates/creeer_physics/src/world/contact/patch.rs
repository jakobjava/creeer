//! A face pressed against a plane (a box's face on a floor, a wall, another box): the
//! material's pressure over the face, in series with the stiffest the step resolves.
//!
//! The face is rigid and flat, so against a plane the depth anywhere on it is the bilinear
//! blend of its four corners' depths δ; over curved ground it is measured at the corners of
//! its triangles (`Domain::Sampled`, `measured.rs`), each pressed by the ground under it, and
//! there the face presses as a punch of the area it presses with. The material presses with
//! p = κ·δ where the face is pressed in, κ making the whole face as stiff as a flat punch of
//! its area, 2·E*·√(A/π) (Johnson, *Contact Mechanics*, 1985, §5.2): κ = 2·E*/√(π·A). A
//! tilted face touches along part of its area only, a part that grows smoothly as it comes
//! down.
//!
//! A step resolves a contact only up to the stiffness m·(Ω/h)² (`mod.rs`), and a face of
//! steel on rock is hundreds of times stiffer. The patch is the material in series with a
//! spring of that stiffness on each way the face moves as a whole (pushed in, tilted about
//! either axis): its energy is
//!
//!   E(δ) = min over σ of ½·σᵀ·L·σ + ½·κ·∫ max(f(δ − B·σ), 0)²,
//!
//! B·σ the corner depths of the face moved rigidly by σ and L the limit for each motion
//! (the limit pushing it in, the limit times J/A tilting it, J the face's second moment).
//! A face pressed in all over is then exactly as stiff as the limit; a face pressed in only
//! near a corner or along an edge, softer than the limit, follows the material: a box on
//! uneven ground sinks the few micrometres its corners would, not the fraction of a
//! millimetre a whole face as soft as the limit would. (Capping κ instead made a crate on
//! terrain sink 0.19 mm onto two diagonal corners and rock on them for seconds, F5.) E is
//! convex, being the infimal convolution of convex energies, and its gradient is the
//! material's force at the minimising depths.

mod material;
mod measured;
mod stiffness;

use super::cap::Cap;

use super::law::FACING;
use super::layer::Layer;
use crate::world::shape::{Domain, Sampled};
use creeer_math::glam::{DMat3, DVec3};
use std::sync::Arc;

/// The face's corner weights at a point (s, t) of it.
pub(super) fn weights([s, t]: [f64; 2]) -> [f64; 4] {
    [(1.0 - s) * (1.0 - t), s * (1.0 - t), s * t, (1.0 - s) * t]
}

/// A face pressed against a plane. Its four corners are consecutive contacts of the group,
/// from `first`. The pressure is integrated over the part of the face that can press (its
/// domain), in triangles fanned from the domain's centroid: an evenly pressed square loads
/// its corners equally. The corners are material points, so their friction anchors stay put
/// from step to step.
#[derive(Clone, Debug, PartialEq)]
pub struct Patch {
    /// Index of the first corner's contact.
    pub first: usize,
    /// Pressure per penetration κ (Pa/m).
    pub modulus: f64,
    /// Twice the contact modulus E*, weighted by how far the face is turned towards the plane
    /// (Pa): κ = punch/√(π·A) for a punch of area A.
    punch: f64,
    /// The face's own area (m²).
    own: f64,
    /// Area of the domain seen along the normal (m²).
    pub area: f64,
    /// The resolvable stiffness of the whole patch (`mod.rs`); none until set (the material
    /// alone, and no matrix yet).
    cap: Option<Cap>,
    /// The domain's triangles: area seen along the normal, and the corners' weights at
    /// their three points.
    triangles: Vec<(f64, [[f64; 4]; 3])>,
    /// Over curved ground, the ground under the triangles' corners (`Domain::Sampled`): the
    /// depth there is measured, not blended from the face's corners (`Self::evaluate_on`).
    sampled: Option<Arc<Sampled>>,
    /// The corner depths of the face moved rigidly: B (rows: corners; columns: pushed in,
    /// tilted along s, tilted along t, about the domain's centroid).
    rigid: [[f64; 3]; 4],
    /// The cap as a layer over the domain, in series with the material.
    layer: Layer,
    /// Where the face's dashpot acts: the midpoints of the triangles' edges, each with a
    /// third of its triangle's area (as the layer, `layer.rs`), the corners' weights there,
    /// and its triangle and the edge's first corner in it.
    pub dashpot: Vec<(f64, [f64; 4], [usize; 2])>,
    /// ∫ NₖNₗ over the domain (m²).
    mass: [[f64; 4]; 4],
    /// ∫ NₖNₗ over each triangle (m²): its share of `mass`, all of it pressed.
    full: Vec<[[f64; 4]; 4]>,
    /// Over curved ground, the depth at each triangle's corners as found (`Patch::new`).
    found: Vec<[f64; 3]>,
    /// The stiffness between the corners for the step's matrices (N/m; `Self::set_cap`, `Self::prepare`).
    matrix: [[f64; 4]; 4],
    /// The face's rigid motion σ at the step's start, the corner depths there, and dσ/dδ
    /// by corner (`Self::prepare`).
    start: (DVec3, [f64; 4], [DVec3; 4]),
}

impl Patch {
    /// A patch of the face with `corners` (in order around it) whose `domain` presses along
    /// `normal`, with contact modulus `modulus` (E*, Pa). The domain is a punch of its own
    /// area; it presses with the area it covers seen along the normal. A face turned towards
    /// the plane by a cosine between [`FACING`] and twice that enters with a weight rising
    /// smoothly from 0 to 1, so the faces taken never jump (`narrow.rs` leaves out those
    /// below).
    pub fn new(
        first: usize,
        corners: [DVec3; 4],
        domain: &Domain,
        normal: DVec3,
        modulus: f64,
    ) -> Self {
        let at = |w: [f64; 4]| (0..4).map(|k| corners[k] * w[k]).sum::<DVec3>();
        // The domain in triangles: as given over curved ground, else fanned from its centre.
        let given: Vec<[[f64; 4]; 3]> = match domain.sampled() {
            Some(sampled) => sampled
                .triangles
                .iter()
                .map(|t| t.map(|i| weights(sampled.vertices[i])))
                .collect(),
            None => {
                let points: Vec<[f64; 4]> = domain.points().iter().copied().map(weights).collect();
                let n = f64::from(u32::try_from(points.len()).expect("a few corners"));
                let centre: [f64; 4] =
                    std::array::from_fn(|k| points.iter().map(|w| w[k]).sum::<f64>() / n);
                (0..points.len())
                    .map(|i| [centre, points[i], points[(i + 1) % points.len()]])
                    .collect()
            }
        };
        let (mut own, mut projected) = (0.0, 0.0);
        let mut triangles = Vec::with_capacity(given.len());
        for w in given {
            let half = (at(w[1]) - at(w[0])).cross(at(w[2]) - at(w[0])) * 0.5;
            let area = half.dot(normal).abs();
            own += half.length();
            projected += area;
            triangles.push((area, w));
        }
        let facing = ((projected / own.max(1e-30) - FACING) / FACING).clamp(0.0, 1.0);
        // The face's coordinates (s, t) at a point from its corner weights.
        let st = |w: &[f64; 4]| [w[1] + w[2], w[2] + w[3]];
        // ∫ g·h over a triangle for g, h linear: A/12·(Σ gⱼhⱼ + Σg·Σh).
        let product = |area: f64, g: [f64; 3], h: [f64; 3]| {
            area / 12.0
                * (g.iter().zip(&h).map(|(a, b)| a * b).sum::<f64>()
                    + g.iter().sum::<f64>() * h.iter().sum::<f64>())
        };
        // The domain's centroid in (s, t): ∫ of a linear function over a triangle is its area
        // times the mean of its values.
        let mut mean = [0.0; 2];
        for &(area, w) in &triangles {
            for (axis, m) in mean.iter_mut().enumerate() {
                *m += area / 3.0 * w.iter().map(|p| st(p)[axis]).sum::<f64>();
            }
        }
        let mean = mean.map(|m| m / projected.max(1e-30));
        let mut mass = [[0.0; 4]; 4];
        for &(area, w) in &triangles {
            for (k, row) in mass.iter_mut().enumerate() {
                for (l, m) in row.iter_mut().enumerate() {
                    *m += product(area, w.map(|p| p[k]), w.map(|p| p[l]));
                }
            }
        }
        let full: Vec<[[f64; 4]; 4]> = triangles
            .iter()
            .map(|&(area, w)| {
                std::array::from_fn(|k| {
                    std::array::from_fn(|l| product(area, w.map(|p| p[k]), w.map(|p| p[l])))
                })
            })
            .collect();
        let corner_st = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let rigid = corner_st.map(|[s, t]| [1.0, s - mean[0], t - mean[1]]);
        let punch = facing * facing * (3.0 - 2.0 * facing) * 2.0 * modulus;
        Self {
            first,
            modulus: punch / (std::f64::consts::PI * own.max(1e-12)).sqrt(),
            punch,
            own,
            area: projected,
            cap: None,
            layer: Layer::new(
                &triangles
                    .iter()
                    .map(|&(area, w)| (area, w.map(|p| st(&p))))
                    .collect::<Vec<_>>(),
                mean,
            ),
            dashpot: triangles
                .iter()
                .enumerate()
                .flat_map(|(t, &(area, w))| {
                    (0..3).map(move |j| {
                        let (p, q) = (w[j], w[(j + 1) % 3]);
                        let middle = std::array::from_fn(|k| 0.5 * (p[k] + q[k]));
                        (area / 3.0, middle, [t, j])
                    })
                })
                .collect(),
            triangles,
            sampled: domain.sampled().cloned(),
            rigid,
            mass,
            full,
            found: domain.sampled().map_or_else(Vec::new, |s| {
                s.triangles
                    .iter()
                    .map(|t| {
                        t.map(|v| {
                            let (foot, n) = s.feet[v];
                            -n.dot(at(weights(s.vertices[v])) - foot)
                        })
                    })
                    .collect()
            }),
            matrix: [[0.0; 4]; 4],
            start: (DVec3::ZERO, [0.0; 4], [DVec3::ZERO; 4]),
        }
    }

    /// Sets the largest stiffness of the patch pushed in (N/m), and with it the stiffness
    /// between its corners for the step's matrices.
    pub fn set_cap(&mut self, cap: Cap) {
        if self.cap == Some(cap) {
            return;
        }
        self.cap = Some(cap);
        self.layer.set(cap, self.area);
        // Over curved ground the face presses as a punch of the area it presses with, at the
        // reach the cap sets (`measured.rs`), before any energy of the step is measured.
        if !self.found.is_empty() {
            let area = self.punch_area(&self.found);
            self.modulus = self.punch / (std::f64::consts::PI * area).sqrt();
        }
        let layer = if self.capped() {
            self.layer.at(DVec3::ZERO).2
        } else {
            DMat3::ZERO
        };
        self.matrix = self.effective(layer);
    }

    /// The stored energy of the patch with its corners pressed in by `depths` (m), and the
    /// force on each corner (N, dE/dδ).
    pub fn evaluate(&self, depths: [f64; 4]) -> (f64, [f64; 4]) {
        let (energy, forces, _) = self.evaluate_on(depths, &[]);
        (energy, forces)
    }

    /// As [`Self::evaluate`], with the depths `measured` at the corners of its triangles over
    /// curved ground (`Sampled::vertices`; empty elsewhere): the energy, the force on each
    /// corner of the face (how the load is shared out to them), and the face's minimising
    /// motion σ, from which the forces at its measured points follow (`Self::point_forces`).
    pub fn evaluate_on(&self, depths: [f64; 4], measured: &[f64]) -> (f64, [f64; 4], DVec3) {
        self.evaluate_from(depths, measured, self.guess(depths))
    }

    /// σ at corner depths `depths` predicted from the step's start.
    pub fn guess(&self, depths: [f64; 4]) -> DVec3 {
        let (sigma, at, slope) = &self.start;
        (0..4).fold(*sigma, |g, k| g + slope[k] * (depths[k] - at[k]))
    }

    /// Finds the face's rigid motion σ at the corner depths of the step's start, and how it
    /// follows the depths there, dσ/dδ = (L + Bᵀ·K·B)⁻¹·Bᵀ·K (from L·σ = Bᵀ·F(δ − B·σ)):
    /// every evaluation of the step starts its search from σ so predicted, one integration
    /// away from the answer (from σ = 0 a face resting on uneven ground took three, and as
    /// many line-search trials, F5). E stays a function of the depths alone within the step.
    pub fn prepare(&mut self, depths: [f64; 4]) {
        self.prepare_on(depths, &[]);
    }

    /// As [`Self::prepare`], with the depths `measured` at its triangles' corners. Over curved
    /// ground the face presses as a punch of the area it presses with (`Self::punch_area`).
    pub fn prepare_on(&mut self, depths: [f64; 4], measured: &[f64]) {
        let offsets = self.offsets(depths, measured);
        let sigma = self.minimise(depths, &offsets, DVec3::ZERO).2;
        let mut slope = [DVec3::ZERO; 4];
        if self.capped() {
            // The layer as loaded at the step's start: the matrix's stiffness too.
            let layer = self.layer.at(sigma).2;
            self.matrix = self.effective(layer);
            let shifted: [f64; 4] =
                std::array::from_fn(|k| depths[k] - DVec3::from(self.rigid[k]).dot(sigma));
            let (_, _, pressed) = self.material(shifted, &offsets);
            let b = |k: usize| DVec3::from(self.rigid[k]);
            // (Bᵀ·K)ₖ, the column of corner k.
            let btk: [DVec3; 4] =
                std::array::from_fn(|k| (0..4).fold(DVec3::ZERO, |s, m| s + b(m) * pressed[m][k]));
            let hessian = (0..4).fold(layer, |h, k| {
                h + DMat3::from_cols(btk[k] * b(k).x, btk[k] * b(k).y, btk[k] * b(k).z)
            });
            let inverse = hessian.inverse();
            slope = btk.map(|c| inverse * c);
        }
        self.start = (sigma, depths, slope);
    }

    /// E at `depths`, its gradient, and the minimising σ, searched from `start`.
    fn minimise(
        &self,
        depths: [f64; 4],
        offsets: &[[f64; 3]],
        start: DVec3,
    ) -> (f64, [f64; 4], DVec3) {
        // Pressed nowhere: on a plane, the depth across the face is a blend of the corners'.
        let nowhere = if offsets.is_empty() {
            depths.iter().all(|&d| d <= 0.0)
        } else {
            (0..self.triangles.len())
                .all(|t| self.depths_at(t, depths, offsets).iter().all(|&d| d <= 0.0))
        };
        if nowhere {
            return (0.0, [0.0; 4], DVec3::ZERO);
        }
        if !self.capped() {
            let (energy, forces, _) = self.material(depths, offsets);
            return (energy, forces, DVec3::ZERO);
        }
        let shifted = |sigma: DVec3| -> [f64; 4] {
            std::array::from_fn(|k| depths[k] - DVec3::from(self.rigid[k]).dot(sigma))
        };
        let total = |sigma: DVec3, material: f64| self.layer.at(sigma).0 + material;
        // Newton on the convex Φ(σ).
        let mut sigma = start;
        let mut at = self.material(shifted(sigma), offsets);
        let mut value = total(sigma, at.0);
        for _ in 0..MAX_NEWTON {
            let (_, forces, pressed) = at;
            // ∇Φ = ∇Ψ − Bᵀ·F, ∇²Φ = ∇²Ψ + κ·Bᵀ·M·B.
            let (_, mut gradient, mut hessian) = self.layer.at(sigma);
            for k in 0..4 {
                let bk = DVec3::from(self.rigid[k]);
                gradient -= bk * forces[k];
                for (m, &mkl) in pressed[k].iter().enumerate() {
                    let bm = DVec3::from(self.rigid[m]);
                    hessian += DMat3::from_cols(bk * bm.x, bk * bm.y, bk * bm.z) * mkl;
                }
            }
            let step = hessian.inverse() * -gradient;
            // The Newton decrement: Φ(σ) − min Φ to second order.
            let decrement = -0.5 * gradient.dot(step);
            if decrement <= DECREMENT * value.abs() + f64::MIN_POSITIVE {
                break;
            }
            // Pressed in all over before and after the step (f is linear in σ, so all along
            // it), Φ is quadratic there and the step lands on its minimum: the forces and the
            // energy follow without integrating again (a face resting on another, the usual
            // case, costs one integration, as it did with κ capped).
            if self.pressed_all_over(shifted(sigma), offsets)
                && self.pressed_all_over(shifted(sigma + step), offsets)
                && self.layer.linear(sigma, sigma + step)
            {
                let moved: [f64; 4] = std::array::from_fn(|m| DVec3::from(self.rigid[m]).dot(step));
                let forces: [f64; 4] = std::array::from_fn(|k| {
                    forces[k] - (0..4).map(|m| pressed[k][m] * moved[m]).sum::<f64>()
                });
                let value = value + gradient.dot(step) + 0.5 * step.dot(hessian * step);
                return (value, forces, sigma + step);
            }
            let mut alpha = 1.0;
            loop {
                let trial = sigma + step * alpha;
                let next = self.material(shifted(trial), offsets);
                let next_value = total(trial, next.0);
                if next_value <= value {
                    (sigma, at, value) = (trial, next, next_value);
                    break;
                }
                // Rejected within the energy's rounding (a cut through a triangle near its
                // corner loses digits): σ is as good as the energy can tell.
                if decrement <= NOISE * value.abs() || alpha < 1e-6 {
                    return (value, at.1, sigma);
                }
                alpha *= 0.5;
            }
        }
        (value, at.1, sigma)
    }

    /// Over curved ground, the ground under its triangles' corners.
    pub fn sampled(&self) -> Option<&Sampled> {
        self.sampled.as_deref()
    }

    /// Whether a finite cap is set.
    fn capped(&self) -> bool {
        self.cap.is_some_and(|c| c.limit.is_finite())
    }
}

/// Newton iterations on the face's rigid motion at most (each one exact on a face pressed
/// in all over; a face pressed in near a corner takes a few).
const MAX_NEWTON: usize = 30;
/// The relative energy error the motion is found to.
const DECREMENT: f64 = 1e-13;
/// The relative energy below which a rejected Newton step is rounding, not a bad step.
const NOISE: f64 = 1e-9;
