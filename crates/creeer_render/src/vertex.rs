//! The mesh vertex as the GPU stores it (40 bytes): where, which way it faces, and what its
//! surface is (G1.3b–c, ADR-0026, `docs/art-style.md` §6).
//!
//! A model's vertex names a palette token and carries what its script baked: occlusion, edge
//! wear, its part's tone, its height above the model's base and its grain coordinates. A
//! procedural vertex ([`MeshVertex::plain`]) takes its instance's own material; a bevelled one
//! ([`MeshVertex::bevelled`]), of a unit primitive, is drawn with its edges bevelled by a radius
//! in metres whatever its instance's scale (`crate::primitives`).

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
    /// occlusion (16–23, 255 open) and edge wear (24–31). A procedural vertex's wear marks it
    /// bevelled (255) or not (0).
    pub material: u32,
    /// Its part's tone (bits 0–7, 128 the token's own) and its height above the model's base
    /// (16–31, over `creeer_art::model::HEIGHT_RANGE`).
    pub detail: u32,
    /// Grain coordinates (m): along its part's grain, and across it.
    pub grain: [f32; 2],
}

/// The token of a vertex drawn in its instance's own material (procedural meshes).
pub const NO_TOKEN: u16 = u16::MAX;

/// Marks the token of a part's vertex (G2.11, ADR-0041): a unit primitive naming a palette token
/// (the token is the bits below), bevelled as a procedural primitive is, its grain and wear found
/// from where it is drawn (`mesh.wgsl`), its tone, seed and taper its instance's
/// ([`crate::Material::part`]).
pub const PART: u16 = 0x8000;

/// How a part's surface follows its wood (`detail` bits 8–15; `mesh.wgsl`'s `GRAIN_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Grain {
    /// Along its Y axis, the pith on the axis: a pole's or a log's wood (rings on its ends).
    Round = 1,
    /// Along its longest side, the pith beside it: a sawn board's (its rings cut across).
    Sawn = 2,
    /// Bark, its furrows along its Y axis: a log's or a branch's side.
    Bark = 3,
    /// No grain (a stone, foliage): the token's pattern over its place.
    Solid = 4,
    /// A model's mass of foliage (`crate::foliage`): its token's colour with its baked occlusion
    /// and tone and the mottling, none of the weathering, grime and wear that leaves have not
    /// (and a forest's many pixels cannot pay for).
    Leaves = 5,
}

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

    /// A procedural vertex of a unit primitive, bevelled: drawn at `p·s + (n − 2p)·r` for its
    /// instance's scale `s` and bevel radius `r` (`crate::primitives::shaped`).
    pub fn bevelled(position: [f32; 3], normal: [f32; 3]) -> Self {
        Self {
            material: pack_material(NO_TOKEN, 1.0, 1.0),
            ..Self::plain(position, normal)
        }
    }

    /// This procedural vertex as a part's (G2.11), bevelled if it was: of `token`, its wood
    /// following `grain`; `marks` a second token drawn over it in patches (a birch's dark
    /// marks), if any.
    pub fn as_part(self, token: u16, grain: Grain, marks: Option<u16>) -> Self {
        assert!(token < PART, "a token below the part's mark");
        let bevelled = if self.is_bevelled() { 1.0 } else { 0.0 };
        Self {
            material: pack_material(PART | token, 1.0, bevelled),
            detail: pack_detail(0.5, 0.0) | u32::from(grain as u8) << 8,
            grain: [marks.map_or(-1.0, f32::from), 0.0],
            ..self
        }
    }

    /// This procedural vertex with the token pattern `pattern` drawn over its instance's colour
    /// (`detail` bits 8–15; `surface.wgsl`'s `PATTERN_*`, 0: none).
    pub fn with_pattern(self, pattern: u8) -> Self {
        Self {
            detail: (self.detail & !0xff00) | u32::from(pattern) << 8,
            ..self
        }
    }

    /// True for a part's vertex ([`MeshVertex::as_part`]).
    pub fn is_part(&self) -> bool {
        let token = self.material & 0xffff;
        token != u32::from(NO_TOKEN) && token & u32::from(PART) != 0
    }

    /// True for a bevelled procedural vertex or part's (their wear marks them).
    pub fn is_bevelled(&self) -> bool {
        (self.material & 0xffff == u32::from(NO_TOKEN) || self.is_part())
            && self.material >> 24 != 0
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
        assert!(!plain.is_bevelled());
        assert!(MeshVertex::bevelled([0.0; 3], [0.0, 1.0, 0.0]).is_bevelled());
        // A model's worn vertex is not bevelled: its wear is wear.
        let worn = MeshVertex {
            material: pack_material(3, 1.0, 1.0),
            ..plain
        };
        assert!(!worn.is_bevelled());
        // A part keeps its primitive's bevel, names its token below the mark, and its grain.
        let rim = MeshVertex::bevelled([0.5; 3], [0.0, 1.0, 0.0]).as_part(5, Grain::Bark, Some(7));
        assert!(rim.is_part() && rim.is_bevelled());
        assert_eq!(rim.material & 0x7fff, 5);
        assert_eq!((rim.detail >> 8) & 0xff, Grain::Bark as u32);
        assert!((rim.grain[0] - 7.0).abs() < 1e-6);
        let sharp = plain.as_part(5, Grain::Sawn, None);
        assert!(sharp.is_part() && !sharp.is_bevelled());
        assert!(sharp.grain[0] < 0.0, "no marks");
    }
}
