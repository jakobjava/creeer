//! What is left of a stem's section where it is cut (`docs/adr/0031-trees.md`, decision 4): the
//! disc less each notch (the half-plane past its depth), and whether it carries what stands on
//! it.
//!
//! The load is the weight above the cut, its line of action meeting the cut's plane at its centre
//! of mass (moved by the wind's moment, if any): an eccentric compression N at point P. Over the
//! section the stress is linear, σ(q) = N·(1/A + eᵀ·J⁻¹·(q − C)), with C the section's centroid,
//! e = P − C and J = ∫(q − C)(q − C)ᵀ dA (equilibrium of force and moment). It fails by
//! EN 1995-1-1's rule for bending with compression along the grain (eq. 6.19):
//! (σ_c/f_c)² + σ_m/f_m ≥ 1, with the green wood's strengths. Past a notch through the stem's
//! middle the weight bears beside what is left, and the tree goes over towards the notch.

use creeer_math::det;
use creeer_math::glam::DVec2;

/// Corners of the disc's polygon.
const CORNERS: usize = 96;

/// A notch: the side it is cut from (rad about the stem, in the tree's frame), and how deep (m).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Notch {
    pub azimuth: f64,
    pub depth: f64,
}

impl Notch {
    /// The unit direction it opens to.
    pub fn outward(&self) -> DVec2 {
        let (s, c) = det::sin_cos(self.azimuth);
        DVec2::new(c, s)
    }
}

/// What is left of the section.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    /// Its area (m²), centroid (m) and ∫(q − C)(q − C)ᵀ dA (m⁴: xx, yy, xy).
    pub area: f64,
    pub centre: DVec2,
    pub second: [f64; 3],
    /// Its outline, counter-clockwise.
    pub corners: Vec<DVec2>,
}

impl Section {
    /// A disc of `radius` (m) less `notches`; none if nothing is left.
    pub fn cut(radius: f64, notches: &[Notch]) -> Option<Self> {
        #[allow(clippy::cast_precision_loss)]
        let mut corners: Vec<DVec2> = (0..CORNERS)
            .map(|k| {
                let (s, c) = det::sin_cos(std::f64::consts::TAU * k as f64 / CORNERS as f64);
                DVec2::new(c, s) * radius
            })
            .collect();
        for n in notches {
            corners = clip(&corners, n.outward(), radius - n.depth);
            if corners.len() < 3 {
                return None;
            }
        }
        let (mut area, mut moment, mut xx, mut yy, mut xy) = (0.0, DVec2::ZERO, 0.0, 0.0, 0.0);
        for (k, a) in corners.iter().enumerate() {
            let b = corners[(k + 1) % corners.len()];
            let cross = a.x * b.y - b.x * a.y;
            area += 0.5 * cross;
            moment += (*a + b) * (cross / 6.0);
            xx += (a.x * a.x + a.x * b.x + b.x * b.x) * cross / 12.0;
            yy += (a.y * a.y + a.y * b.y + b.y * b.y) * cross / 12.0;
            xy += (a.x * b.y + 2.0 * a.x * a.y + 2.0 * b.x * b.y + b.x * a.y) * cross / 24.0;
        }
        if area <= 1e-6 * radius * radius {
            return None;
        }
        let centre = moment / area;
        Some(Self {
            area,
            centre,
            second: [
                xx - area * centre.x * centre.x,
                yy - area * centre.y * centre.y,
                xy - area * centre.x * centre.y,
            ],
            corners,
        })
    }

    /// J⁻¹·e: how the stress rises across the section per unit load at `at` (1/m³).
    fn slope(&self, at: DVec2) -> DVec2 {
        let [xx, yy, xy] = self.second;
        let det = xx * yy - xy * xy;
        let e = at - self.centre;
        DVec2::new(yy * e.x - xy * e.y, xx * e.y - xy * e.x) / det
    }

    /// Its use under compression `axial` (N) along the stem, its line through `at`, with the
    /// wood's bending and compressive strengths (Pa): ≥ 1 fails (EN 1995-1-1 eq. 6.19).
    pub fn utilisation(&self, axial: f64, at: DVec2, bending: f64, compression: f64) -> f64 {
        let g = self.slope(at);
        let most = (self.corners.iter())
            .map(|q| g.dot(*q - self.centre))
            .fold(0.0, f64::max);
        let (sc, sm) = (axial / self.area, axial * most);
        (sc / compression) * (sc / compression) + sm / bending
    }

    /// The corner bearing most under a load through `at`: where it pivots when it goes.
    pub fn bearing(&self, at: DVec2) -> DVec2 {
        let g = self.slope(at);
        (self.corners.iter().copied())
            .max_by(|a, b| g.dot(*a).total_cmp(&g.dot(*b)))
            .unwrap_or(self.centre)
    }
}

/// `polygon` clipped to the half-plane x·n ≤ d (Sutherland–Hodgman).
fn clip(polygon: &[DVec2], n: DVec2, d: f64) -> Vec<DVec2> {
    let mut out = Vec::with_capacity(polygon.len() + 1);
    for (k, a) in polygon.iter().enumerate() {
        let b = polygon[(k + 1) % polygon.len()];
        let (fa, fb) = (a.dot(n) - d, b.dot(n) - d);
        if fa <= 0.0 {
            out.push(*a);
        }
        if (fa < 0.0) != (fb < 0.0) {
            out.push(*a + (b - *a) * (fa / (fa - fb)));
        }
    }
    out
}
