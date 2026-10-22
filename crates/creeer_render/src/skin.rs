//! Skinned models (G1.3e, ADR-0026 decision 6): a character's rest mesh moved by its bones,
//! the ragdoll's fifteen segments, with dual quaternions (Kavan et al. 2007). Each bone is a
//! rigid motion; a vertex's bones' motions are blended as unit dual quaternions, so a bent joint
//! keeps its volume where linear blending (the matrices' weighted sum) shrinks it.
//!
//! This is the arithmetic, on the CPU, as `skin.wgsl` does it on the GPU: the shader is tested
//! against it, and it against rigid motions.

use creeer_art::model::Model;
use creeer_art::model_skin::{LEVELS, Part};
use creeer_math::glam::{Quat, Vec3, Vec4};
use std::ops::Range;

/// A skinned model registered with the renderer (`Renderer::add_skinned`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SkinnedId(pub(crate) u32);

/// An outfit of a skinned model: the parts it draws, merged (`Renderer::add_outfit`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OutfitId(pub(crate) u32);

/// A rigid motion as a unit dual quaternion: `real` the rotation, `dual` half the translation
/// times it (q_d = ½·t·q_r).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DualQuat {
    pub real: Quat,
    pub dual: Quat,
}

impl DualQuat {
    /// The motion that turns by `rotation`, then moves by `translation`.
    pub fn from_rigid(rotation: Quat, translation: Vec3) -> Self {
        let t = Quat::from_xyzw(translation.x, translation.y, translation.z, 0.0);
        Self {
            real: rotation,
            dual: (t * rotation) * 0.5,
        }
    }

    /// Its translation.
    pub fn translation(&self) -> Vec3 {
        let t = (self.dual * 2.0) * self.real.conjugate();
        Vec3::new(t.x, t.y, t.z)
    }

    /// The blend of `bones` (each with its weight), turned to its unit dual quaternion: each
    /// taken on the same side as the first (q and −q are one rotation; blending across them
    /// would swing the long way round).
    pub fn blend(bones: &[(Self, f32)]) -> Self {
        let Some((first, _)) = bones.first() else {
            return Self::from_rigid(Quat::IDENTITY, Vec3::ZERO);
        };
        let pivot = Vec4::from(first.real);
        let (mut real, mut dual) = (Vec4::ZERO, Vec4::ZERO);
        for (dq, w) in bones {
            let w = if Vec4::from(dq.real).dot(pivot) < 0.0 {
                -*w
            } else {
                *w
            };
            real += Vec4::from(dq.real) * w;
            dual += Vec4::from(dq.dual) * w;
        }
        let length = real.length();
        Self {
            real: Quat::from_vec4(real / length),
            dual: Quat::from_vec4(dual / length),
        }
    }

    /// Moves point `p` (the real part unit, as [`Self::blend`] leaves it).
    pub fn point(&self, p: Vec3) -> Vec3 {
        let (r, d) = (self.real, self.dual);
        let (rv, dv) = (Vec3::new(r.x, r.y, r.z), Vec3::new(d.x, d.y, d.z));
        p + 2.0 * rv.cross(rv.cross(p) + r.w * p) + 2.0 * (r.w * dv - d.w * rv + rv.cross(dv))
    }

    /// Turns direction `n`.
    pub fn direction(&self, n: Vec3) -> Vec3 {
        let r = self.real;
        let rv = Vec3::new(r.x, r.y, r.z);
        n + 2.0 * rv.cross(rv.cross(n) + r.w * n)
    }
}

/// Linear blend skinning of `p` (the matrices' weighted sum), for comparison: what dual
/// quaternions are taken over.
pub fn linear_blend(bones: &[((Quat, Vec3), f32)], p: Vec3) -> Vec3 {
    bones
        .iter()
        .map(|((rotation, translation), w)| (*rotation * p + *translation) * *w)
        .sum()
}

/// A unit vector in 16-bit signed normalised components.
pub fn pack_normal(n: [f32; 3]) -> [i16; 4] {
    #[allow(clippy::cast_possible_truncation)] // within ±32767
    let q = |x: f32| (x.clamp(-1.0, 1.0) * 32767.0).round() as i16;
    [q(n[0]), q(n[1]), q(n[2]), 0]
}

/// Weights summing to one as bytes summing to 255, each rounded down and the remainder given to
/// the largest fractions (so a vertex on one bone stays wholly on it).
pub fn pack_weights(w: [f32; 4]) -> [u8; 4] {
    let total: f32 = w.iter().sum();
    let scaled = w.map(|x| if total > 0.0 { x / total * 255.0 } else { 0.0 });
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0..=255
    let mut out = scaled.map(|x| x.floor() as u8);
    let given: u32 = out.iter().map(|&b| u32::from(b)).sum();
    let mut order = [0, 1, 2, 3];
    order.sort_by(|&a, &b| {
        (scaled[b] - scaled[b].floor()).total_cmp(&(scaled[a] - scaled[a].floor()))
    });
    for &k in order.iter().take(255usize.saturating_sub(given as usize)) {
        out[k] += 1;
    }
    out
}

/// A skinned model as the renderer takes it: its vertices grouped by level
/// (`creeer_art::model_levels`), each level's error against the close-up one, and its parts.
#[derive(Clone, Debug)]
pub struct SkinnedMeshData {
    pub model: Model,
    /// Each level's vertices.
    pub levels: [Range<u32>; LEVELS as usize],
    /// Each level's error against the close-up one (m): a level is drawn where it shows under
    /// half a pixel.
    pub errors: [f32; LEVELS as usize],
    /// The bones' count (each vertex names up to four).
    pub bones: usize,
    /// A sphere about the model at rest (its centre, model space; its radius), grown by the
    /// largest error, for culling.
    pub centre: Vec3,
    pub radius: f32,
}

impl SkinnedMeshData {
    /// `model` (skinned) with its levels' `errors` (the manifest's).
    pub fn new(model: &Model, errors: [f32; LEVELS as usize]) -> Self {
        let (model, levels) = creeer_art::model_levels::grouped(model);
        let (lo, hi) = model.bounds();
        let centre = 0.5 * (lo + hi);
        let radius = model
            .vertices
            .iter()
            .map(|v| (Vec3::from(v.position) - centre).length())
            .fold(0.0, f32::max)
            + errors.iter().copied().fold(0.0, f32::max);
        Self {
            bones: model.bones.len(),
            model,
            levels,
            errors,
            centre,
            radius,
        }
    }

    /// One index list a level for the parts `shown` (an outfit's visible parts, `wearable::
    /// visible`), each counted from its level's first vertex.
    pub fn outfit(&self, shown: &[&Part]) -> [Vec<u32>; LEVELS as usize] {
        let mut out: [Vec<u32>; LEVELS as usize] = Default::default();
        for part in shown {
            let level = usize::from(part.level);
            let first = self.levels[level].start;
            let indices = &self.model.indices[part.first..part.first + part.count];
            out[level].extend(indices.iter().map(|i| i - first));
        }
        out
    }

    /// The level drawn `distance` metres away (to the model's nearest point) at `focal` pixels
    /// for a unit length a metre away: the coarsest whose error shows under half a pixel.
    pub fn level(&self, distance: f32, focal: f32) -> usize {
        let distance = distance.max(1e-3);
        (0..LEVELS as usize)
            .rev()
            .find(|&l| self.errors[l] * focal / distance < 0.5)
            .unwrap_or(0)
    }
}

#[cfg(test)]
#[path = "skin_tests.rs"]
mod tests;
