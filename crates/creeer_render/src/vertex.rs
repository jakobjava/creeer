//! The mesh vertex as the GPU stores it (40 bytes): where, which way it faces, and what its
//! surface is (G1.3b–c, ADR-0026, `docs/art-style.md` §6).
//!
//! A model's vertex names a palette token and carries what its script baked: occlusion, edge
//! wear, its part's tone, its height above the model's base and its grain coordinates. A
//! procedural vertex ([`MeshVertex::plain`]) takes its instance's own material.

use bytemuck::{Pod, Zeroable};

/// One mesh vertex.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct MeshVertex {
    /// Position in mesh space (m).
    pub position: [f32; 3],
    /// Unit normal in mesh space.
    pub normal: [f32; 3],
    /// Its palette token (bits 0–15, [`NO_TOKEN`] for the instance's own material), baked
    /// occlusion (16–23, 255 open) and edge wear (24–31).
    pub material: u32,
    /// Its part's tone (bits 0–7, 128 the token's own) and its height above the model's base
    /// (16–31, over `creeer_art::model::HEIGHT_RANGE`).
    pub detail: u32,
    /// Grain coordinates (m): along its part's grain, and across it.
    pub grain: [f32; 2],
}

/// The token of a vertex drawn in its instance's own material (procedural meshes).
pub const NO_TOKEN: u16 = u16::MAX;

/// `x` (0–1) in `bits` bits, rounded.
pub(crate) fn quantise(x: f32, bits: u32) -> u32 {
    #[allow(clippy::cast_precision_loss)] // at most 16 bits
    let top = ((1_u32 << bits) - 1) as f32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0..=top
    let q = (x.clamp(0.0, 1.0) * top).round() as u32;
    q
}

/// Packs a vertex's token, occlusion and wear (both 0–1).
pub fn pack_material(token: u16, occlusion: f32, wear: f32) -> u32 {
    u32::from(token) | quantise(occlusion, 8) << 16 | quantise(wear, 8) << 24
}

/// Packs a vertex's part tone (0–1) and height (0–1 of the height range).
pub fn pack_detail(tone: f32, height: f32) -> u32 {
    quantise(tone, 8) | quantise(height, 16) << 16
}

impl MeshVertex {
    /// A procedural vertex, drawn in its instance's own material: open, unworn, of its token's
    /// own tone, at the base, with no grain.
    pub fn plain(position: [f32; 3], normal: [f32; 3]) -> Self {
        Self {
            position,
            normal,
            material: pack_material(NO_TOKEN, 1.0, 0.0),
            detail: pack_detail(0.5, 0.0),
            grain: [0.0; 2],
        }
    }

    /// A model's vertex, its token an index into the palette the renderer holds.
    pub fn from_model(v: &creeer_art::model::Vertex) -> Self {
        Self {
            position: v.position,
            normal: v.normal,
            material: pack_material(v.token, v.occlusion, v.wear),
            detail: pack_detail(v.tone, v.height),
            grain: v.grain,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packing_keeps_each_field_in_its_bits() {
        let m = pack_material(7, 1.0, 0.5);
        assert_eq!(m & 0xffff, 7);
        assert_eq!((m >> 16) & 0xff, 255);
        assert_eq!(m >> 24, 128);
        let d = pack_detail(0.0, 1.0);
        assert_eq!(d & 0xff, 0);
        assert_eq!(d >> 16, 0xffff);
        // Height to 16 bits: 2 m over 65535 steps, 0.03 mm.
        let h = pack_detail(0.5, 0.25) >> 16;
        assert!((f64::from(h) / 65535.0 - 0.25).abs() < 1e-5);
        assert_eq!(std::mem::size_of::<MeshVertex>(), 40);
        let plain = MeshVertex::plain([0.0; 3], [0.0, 1.0, 0.0]);
        assert_eq!(plain.material & 0xffff, u32::from(NO_TOKEN));
    }
}
