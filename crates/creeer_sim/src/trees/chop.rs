//! Chopping (`docs/adr/0031-trees.md`, decision 5): what a blow of an edge takes out of wood, and
//! the notch it makes.
//!
//! **The law.** An edge driven across the grain wedges the fibres apart: its faces press the
//! wood at its strength across the grain σ⊥, so driving it a depth p takes work
//! ≈ L·σ⊥·(tan(θ/2) + μ)·p² (θ the edge's included angle, μ friction on its faces, L its
//! length), and the chip a blow frees is about L·p² (cuts alternating from above and from the
//! side). So the wood a blow of energy E takes out is V = k·E / (σ⊥·(tan(θ/2) + μ)), the same
//! for every edge: a steel axe and a flake bound to a stick differ in their angle and in the
//! energy their swing carries, not by a table of tools. k is calibrated once (`CHIP`), against
//! measured felling; nothing without an edge cuts wood.
//!
//! **The notch** an axe makes has a level floor and a roof at 45° (`OPENING`): `depth` m into a
//! stem of radius R it holds tan 45°·∫ (x − (R − depth))·2√(R² − x²) dx over the chord past
//! R − depth. Blows from about the same side (within 45°) deepen one notch; from elsewhere they
//! start another.

use super::section::Notch;
use crate::items::Edge;
use creeer_math::det;

/// A notch's opening (rad): its roof's angle from its floor.
pub const OPENING: f64 = std::f64::consts::FRAC_PI_4;
/// Friction of wood on an edge's faces.
pub const FRICTION: f64 = 0.3;
/// The chip a blow frees, over the wedge's work. Calibrated (ADR-0031): a steel felling axe
/// (`tools.ron`) fells a 30 cm spruce in about 150 blows, 5 minutes at 30 a minute, as stone
/// axes' measured 20 minutes over their 4–4.5 times steel's on large trees (Moore's synthesis
/// of New Guinea's) and axemen's 25–35 strokes a minute (Glaser, Unasylva 1954) give.
pub const CHIP: f64 = 0.26;
/// Blows within this of a notch's side deepen it (rad).
const SAME_SIDE: f64 = std::f64::consts::FRAC_PI_4;
/// Notches a cut has at most.
const MOST_NOTCHES: usize = 4;

/// The volume of wood (m³) a blow of `energy` (J) with `edge` takes out of wood of strength
/// `across` (Pa) across the grain.
pub fn chopped(energy: f64, edge: Option<Edge>, across: f64) -> f64 {
    let Some(edge) = edge else { return 0.0 };
    if !(energy > 0.0 && across > 0.0) {
        return 0.0;
    }
    CHIP * energy / (across * (det::tan(0.5 * edge.angle) + FRICTION))
}

/// The wood a notch `depth` m deep into a stem of `radius` holds (m³).
pub fn notch_volume(radius: f64, depth: f64) -> f64 {
    let depth = depth.clamp(0.0, 2.0 * radius);
    if depth <= 0.0 {
        return 0.0;
    }
    // ∫ₐᴿ (x − a)·2√(R² − x²) dx = ⅔(R² − a²)^{3/2} − a·(R²·acos(a/R) − a·√(R² − a²)).
    let (r, a) = (radius, radius - depth);
    let chord = (r * r - a * a).max(0.0).sqrt();
    let segment = r * r * det::acos((a / r).clamp(-1.0, 1.0)) - a * chord;
    det::tan(OPENING) * (2.0 / 3.0 * chord * chord * chord - a * segment)
}

/// How deep a notch holding `volume` m³ goes into a stem of `radius` (m).
pub fn depth_of(radius: f64, volume: f64) -> f64 {
    let (mut lo, mut hi) = (0.0, 2.0 * radius);
    if volume >= notch_volume(radius, hi) {
        return hi;
    }
    for _ in 0..50 {
        let mid = 0.5 * (lo + hi);
        if notch_volume(radius, mid) < volume {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// A cut into a stem: at a height (m from the tree's base), its notches and the wood taken out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cut {
    pub z: f64,
    /// Each notch's side (rad about the stem) and the wood taken out of it (m³).
    pub notches: Vec<(f64, f64)>,
}

impl Cut {
    /// A cut begun at height `z`.
    pub fn at(z: f64) -> Self {
        Self {
            z,
            notches: Vec::new(),
        }
    }

    /// `volume` m³ more taken out from side `azimuth` (rad about the stem).
    pub fn chop(&mut self, azimuth: f64, volume: f64) {
        let nearest = (0..self.notches.len()).min_by(|a, b| {
            let off = |k: usize| angle_between(self.notches[k].0, azimuth);
            off(*a).total_cmp(&off(*b))
        });
        match nearest {
            Some(k)
                if angle_between(self.notches[k].0, azimuth) <= SAME_SIDE
                    || self.notches.len() >= MOST_NOTCHES =>
            {
                self.notches[k].1 += volume;
            }
            _ => self.notches.push((azimuth, volume)),
        }
    }

    /// Its notches in a stem of `radius` (m).
    pub fn notches(&self, radius: f64) -> Vec<Notch> {
        (self.notches.iter())
            .map(|&(azimuth, v)| Notch {
                azimuth,
                depth: depth_of(radius, v),
            })
            .collect()
    }

    /// The wood taken out in all (m³).
    pub fn removed(&self) -> f64 {
        self.notches.iter().map(|n| n.1).sum()
    }
}

/// The angle between two directions about the stem (rad, 0 to π).
pub fn angle_between(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(std::f64::consts::TAU);
    d.min(std::f64::consts::TAU - d)
}
