//! What skinning reads and writes, packed (G1.3e): a skinned frame of eight characters close up
//! moves about half a million vertices, and its cost is their bytes. Only what moves is skinned
//! (place, normal, and the material, whose per-player tokens become the character's own); what
//! does not (tone, height, grain) is drawn straight from the rest data as a second stream.

use super::mesh_pass::instanced_vertex_layouts;
use crate::skin::{pack_normal, pack_weights};
use crate::vertex::MeshVertex;
use bytemuck::{Pod, Zeroable};

/// A rest vertex as `skin.wgsl` reads it (8 words): place, normal (16-bit), material, its four
/// bones (a byte each) and their weights (8-bit, summing to 255).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub(super) struct RestRaw {
    pub position: [f32; 3],
    pub normal: [i16; 4],
    pub material: u32,
    pub bones: [u8; 4],
    pub weights: [u8; 4],
}

/// What a skinned vertex keeps from rest, drawn as it is (3 words): `MeshVertex`'s detail and
/// grain.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub(super) struct StaticRaw {
    pub detail: u32,
    pub grain: [f32; 2],
}

/// A skinned vertex as `skin.wgsl` writes it (6 words): place, normal (16-bit), material.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct SkinnedVertex {
    pub position: [f32; 3],
    pub normal: [i16; 4],
    pub material: u32,
}

impl SkinnedVertex {
    /// Its normal, unpacked.
    pub fn normal(&self) -> [f32; 3] {
        [0, 1, 2].map(|i| f32::from(self.normal[i]) / 32767.0)
    }
}

pub(super) const SKINNED_BYTES: u64 = std::mem::size_of::<SkinnedVertex>() as u64;
pub(super) const STATIC_BYTES: u64 = std::mem::size_of::<StaticRaw>() as u64;

/// A model's vertex split into what is skinned and what is not.
pub(super) fn split(v: &creeer_art::model::Vertex) -> (RestRaw, StaticRaw) {
    let mesh = MeshVertex::from_model(v);
    (
        RestRaw {
            position: v.position,
            normal: pack_normal(v.normal),
            material: mesh.material,
            bones: v.bones,
            weights: pack_weights(v.weights),
        },
        StaticRaw {
            detail: mesh.detail,
            grain: mesh.grain,
        },
    )
}

const SKINNED_ATTRIBUTES: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Snorm16x4, 6 => Uint32];
const STATIC_ATTRIBUTES: [wgpu::VertexAttribute; 2] =
    wgpu::vertex_attr_array![8 => Uint32, 9 => Float32x2];

/// The vertex buffers a skinned draw binds: the skinned vertices, the instances (as every mesh's)
/// and the rest data's static part. The mesh shaders read them as they read a model's.
pub(super) fn skinned_vertex_layouts() -> [Option<wgpu::VertexBufferLayout<'static>>; 3] {
    let [_, instances] = instanced_vertex_layouts();
    [
        Some(wgpu::VertexBufferLayout {
            array_stride: SKINNED_BYTES,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &SKINNED_ATTRIBUTES,
        }),
        instances,
        Some(wgpu::VertexBufferLayout {
            array_stride: STATIC_BYTES,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &STATIC_ATTRIBUTES,
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skin::pack_weights;

    #[test]
    fn packed_weights_sum_to_255_and_keep_one_bone_whole() {
        assert_eq!(pack_weights([1.0, 0.0, 0.0, 0.0]), [255, 0, 0, 0]);
        let w = pack_weights([0.5, 0.25, 0.125, 0.125]);
        assert_eq!(w.iter().map(|&b| u32::from(b)).sum::<u32>(), 255);
        for (packed, exact) in w.iter().zip([0.5f32, 0.25, 0.125, 0.125]) {
            assert!((f32::from(*packed) / 255.0 - exact).abs() <= 1.0 / 255.0);
        }
        assert_eq!(std::mem::size_of::<RestRaw>(), 32);
        assert_eq!(SKINNED_BYTES, 24);
        assert_eq!(STATIC_BYTES, 12);
    }
}
