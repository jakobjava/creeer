//! Classical orbital elements ↔ state vectors.
//!
//! The elements use the semi-latus rectum p instead of the semi-major axis a, so the parabola
//! (a = ∞) needs no special case. The angles are defined through an in-plane basis that
//! falls back to the x axis when the node is undefined (equatorial orbits) and to the node
//! when the periapsis is undefined (circular orbits): every state has elements, and turning
//! the elements back gives the same state.

use super::{eccentric_anomaly, from_textbook, hyperbolic_anomaly, to_textbook};
use creeer_math::det;
use creeer_math::glam::DVec3;
use std::f64::consts::TAU;

/// Below this the node or the periapsis is taken as undefined.
const DEGENERATE: f64 = 1e-12;

/// Osculating orbital elements about a point mass.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Elements {
    /// Gravitational parameter of the central body (m³/s²).
    pub mu: f64,
    /// Semi-latus rectum p = h²/μ (m).
    pub p: f64,
    /// Eccentricity.
    pub e: f64,
    /// Inclination against the reference plane (rad, 0..π; > π/2 is retrograde).
    pub inclination: f64,
    /// Longitude of the ascending node Ω (rad).
    pub node: f64,
    /// Argument of periapsis ω (rad).
    pub periapsis_argument: f64,
    /// True anomaly ν (rad).
    pub true_anomaly: f64,
}

impl Elements {
    /// Elements of an orbit given by its semi-major axis (m) instead of p; `true_anomaly` in
    /// radians. Hyperbolas take a negative `a`.
    pub fn from_semi_major_axis(
        mu: f64,
        a: f64,
        e: f64,
        [inclination, node, periapsis_argument, true_anomaly]: [f64; 4],
    ) -> Self {
        Self {
            mu,
            p: a * (1.0 - e * e),
            e,
            inclination,
            node,
            periapsis_argument,
            true_anomaly,
        }
    }

    /// The osculating elements of a state (position and velocity relative to the central
    /// body).
    pub fn from_state(mu: f64, r: DVec3, v: DVec3) -> Self {
        let (r, v) = (to_textbook(r), to_textbook(v));
        let h = r.cross(v);
        let hn = h.length();
        let h_hat = h / hn;
        let rn = r.length();
        let e_vec = (r * (v.length_squared() - mu / rn) - v * r.dot(v)) / mu;
        let e = e_vec.length();
        let (node_hat, node) = node_basis(h_hat);
        let q1 = h_hat.cross(node_hat);
        let periapsis_argument = if e > DEGENERATE {
            det::atan2(e_vec.dot(q1), e_vec.dot(node_hat)).rem_euclid(TAU)
        } else {
            0.0
        };
        let (p_hat, q_hat) = periapsis_basis(h_hat, node_hat, q1, periapsis_argument);
        Self {
            mu,
            p: hn * hn / mu,
            e,
            inclination: det::acos((h_hat.z).clamp(-1.0, 1.0)),
            node,
            periapsis_argument,
            true_anomaly: det::atan2(r.dot(q_hat), r.dot(p_hat)).rem_euclid(TAU),
        }
    }

    /// Position and velocity relative to the central body.
    pub fn to_state(&self) -> (DVec3, DVec3) {
        let (si, ci) = det::sin_cos(self.inclination);
        let (sn, cn) = det::sin_cos(self.node);
        let h_hat = DVec3::new(si * sn, -si * cn, ci);
        let node_hat = DVec3::new(cn, sn, 0.0);
        let q1 = h_hat.cross(node_hat);
        let (p_hat, q_hat) = periapsis_basis(h_hat, node_hat, q1, self.periapsis_argument);
        let (s, c) = det::sin_cos(self.true_anomaly);
        let rn = self.p / (1.0 + self.e * c);
        let r = (p_hat * c + q_hat * s) * rn;
        let v = (p_hat * -s + q_hat * (self.e + c)) * (self.mu / self.p).sqrt();
        (from_textbook(r), from_textbook(v))
    }

    /// Semi-major axis (m): negative for hyperbolas, infinite for the parabola.
    pub fn semi_major_axis(&self) -> f64 {
        self.p / (1.0 - self.e * self.e)
    }

    /// Closest distance to the centre (m).
    pub fn periapsis(&self) -> f64 {
        self.p / (1.0 + self.e)
    }

    /// Farthest distance (m); infinite for open orbits.
    pub fn apoapsis(&self) -> f64 {
        if self.e < 1.0 {
            self.p / (1.0 - self.e)
        } else {
            f64::INFINITY
        }
    }

    /// Orbital period (s); infinite for open orbits.
    pub fn period(&self) -> f64 {
        if self.e < 1.0 {
            let a = self.semi_major_axis();
            TAU * (a * a * a / self.mu).sqrt()
        } else {
            f64::INFINITY
        }
    }

    /// Specific orbital energy (J/kg): −μ/2a.
    pub fn energy(&self) -> f64 {
        -0.5 * self.mu * (1.0 - self.e * self.e) / self.p
    }

    /// Mean anomaly (rad) for e ≠ 1: elliptic M = E − e·sin E, hyperbolic M = e·sinh H − H.
    pub fn mean_anomaly(&self) -> f64 {
        let (s, c) = det::sin_cos(self.true_anomaly);
        if self.e < 1.0 {
            let big_e = det::atan2((1.0 - self.e * self.e).sqrt() * s, self.e + c);
            (big_e - self.e * det::sin(big_e)).rem_euclid(TAU)
        } else {
            // tanh(H/2) = √((e−1)/(e+1))·tan(ν/2), written without the tangent.
            let x = ((self.e - 1.0) / (self.e + 1.0)).sqrt() * s / (1.0 + c);
            let h = det::ln_1p(2.0 * x / (1.0 - x)); // 2·atanh(x)
            self.e * det::sinh(h) - h
        }
    }

    /// The same orbit at mean anomaly `mean` (e ≠ 1).
    pub fn with_mean_anomaly(mut self, mean: f64) -> Self {
        self.true_anomaly = if self.e < 1.0 {
            let big_e = eccentric_anomaly(mean, self.e);
            let (s, c) = det::sin_cos(big_e);
            det::atan2((1.0 - self.e * self.e).sqrt() * s, c - self.e).rem_euclid(TAU)
        } else {
            let h = hyperbolic_anomaly(mean, self.e);
            let (sh, ch) = (det::sinh(h), det::cosh(h));
            det::atan2((self.e * self.e - 1.0).sqrt() * sh, self.e - ch)
        };
        self
    }
}

/// The ascending node's direction and longitude; the x axis for equatorial orbits.
fn node_basis(h_hat: DVec3) -> (DVec3, f64) {
    let n = DVec3::new(-h_hat.y, h_hat.x, 0.0); // z × h
    let nn = n.length();
    if nn > DEGENERATE {
        let n = n / nn;
        (n, det::atan2(n.y, n.x).rem_euclid(TAU))
    } else {
        (DVec3::X, 0.0)
    }
}

/// Periapsis direction and the in-plane direction 90° ahead of it.
fn periapsis_basis(h_hat: DVec3, node_hat: DVec3, q1: DVec3, argument: f64) -> (DVec3, DVec3) {
    let (s, c) = det::sin_cos(argument);
    let p_hat = node_hat * c + q1 * s;
    (p_hat, h_hat.cross(p_hat))
}
