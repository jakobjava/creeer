//! Terrain chunks: GPU storage, culling and drawing (camera view and shadow cascades).
//!
//! Each chunk has its own vertex and index buffer (chunks come and go as the camera moves);
//! one per-frame instance buffer holds the camera-relative origin of every chunk drawn, first
//! for the camera, then for each shadow cascade.

use super::*;
use crate::shadow::LightBasis;
use crate::terrain::{TerrainMeshData, TerrainVertex};
use creeer_math::glam::{DVec3, Vec3};
use std::collections::HashMap;
use wgpu::util::DeviceExt;

const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Snorm8x4, 2 => Unorm8x4];
const INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![3 => Float32x4];

fn vertex_layouts() -> [Option<wgpu::VertexBufferLayout<'static>>; 2] {
    [
        Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<TerrainVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &VERTEX_ATTRIBUTES,
        }),
        Some(wgpu::VertexBufferLayout {
            array_stride: 16,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &INSTANCE_ATTRIBUTES,
        }),
    ]
}

/// One uploaded chunk.
struct GpuChunk {
    origin: DVec3,
    center: Vec3,
    radius: f32,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    /// Indices of the surface; then the skirts of each face (start, end).
    surface: u32,
    skirts: [(u32, u32); 6],
}

/// Terrain chunks and the pipelines that draw them.
pub(super) struct TerrainPass {
    pipeline: wgpu::RenderPipeline,
    shadow_pipeline: wgpu::RenderPipeline,
    chunks: HashMap<u64, GpuChunk>,
    instances: wgpu::Buffer,
    instance_capacity: usize,
    /// (chunk, instance, skirt faces) drawn for the camera, then per cascade.
    draws: Vec<(u64, u32, u8)>,
    shadow_draws: [Vec<(u64, u32, u8)>; CASCADES],
    /// Triangles drawn for the camera in the last frame.
    pub(super) triangles: usize,
}

impl TerrainPass {
    pub(super) fn new(
        device: &wgpu::Device,
        globals_layout: &wgpu::BindGroupLayout,
        shadow_sample_layout: &wgpu::BindGroupLayout,
        luts_layout: &wgpu::BindGroupLayout,
        cascade_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let source = concat!(
            include_str!("../shaders/atmosphere.wgsl"),
            "\n",
            include_str!("../shaders/lighting.wgsl"),
            "\n",
            include_str!("../shaders/terrain.wgsl")
        );
        let module = shader(device, "terrain", source);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("terrain"),
            bind_group_layouts: &[
                Some(globals_layout),
                Some(shadow_sample_layout),
                Some(luts_layout),
            ],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("terrain"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &vertex_layouts(),
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: scene_depth_state(),
            multisample: scene_multisample(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(HDR_FORMAT.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
        let shadow_module = shader(
            device,
            "terrain shadow",
            include_str!("../shaders/terrain_shadow.wgsl"),
        );
        let shadow_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("terrain shadow"),
            bind_group_layouts: &[Some(globals_layout), Some(cascade_layout)],
            immediate_size: 0,
        });
        let shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("terrain shadow"),
            layout: Some(&shadow_layout),
            vertex: wgpu::VertexState {
                module: &shadow_module,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &vertex_layouts(),
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(super::shadow_pass::shadow_depth_state()),
            multisample: wgpu::MultisampleState::default(),
            fragment: None,
            multiview_mask: None,
            cache: None,
        });
        Self {
            pipeline,
            shadow_pipeline,
            chunks: HashMap::new(),
            instances: instance_buffer(device, 256),
            instance_capacity: 256,
            draws: Vec::new(),
            shadow_draws: Default::default(),
            triangles: 0,
        }
    }

    pub(super) fn insert(
        &mut self,
        device: &wgpu::Device,
        id: u64,
        origin: DVec3,
        mesh: &TerrainMeshData,
    ) {
        if mesh.indices.is_empty() {
            self.chunks.remove(&id);
            return;
        }
        let (center, radius) = mesh.bounds();
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("terrain vertices"),
            contents: bytemuck::cast_slice(&mesh.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("terrain indices"),
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let surface = mesh
            .surface_indices
            .min(u32::try_from(mesh.indices.len()).expect("chunk mesh too large"));
        self.chunks.insert(
            id,
            GpuChunk {
                origin,
                center,
                radius,
                vertices,
                indices,
                surface,
                skirts: mesh.skirts,
            },
        );
    }

    pub(super) fn remove(&mut self, id: u64) {
        self.chunks.remove(&id);
    }

    pub(super) fn len(&self) -> usize {
        self.chunks.len()
    }

    /// Chunks drawn for the camera in the last frame.
    pub(super) fn drawn(&self) -> usize {
        self.draws.len()
    }

    /// Culls the chunks `ids` against the camera and the cascades and uploads the instances.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        ids: &[(u64, u8)],
        camera: DVec3,
        frustum: &Frustum,
        shadows: Option<(&[Cascade; CASCADES], &LightBasis)>,
    ) {
        let mut origins: Vec<[f32; 4]> = Vec::new();
        self.draws.clear();
        for draws in &mut self.shadow_draws {
            draws.clear();
        }
        self.triangles = 0;
        let mut per_cascade: [Vec<(u64, u8, [f32; 4])>; CASCADES] = Default::default();
        for &(id, skirts) in ids {
            let Some(chunk) = self.chunks.get(&id) else {
                continue;
            };
            let origin = (chunk.origin - camera).as_vec3();
            let center = origin + chunk.center;
            if frustum.intersects_sphere(center, chunk.radius) {
                let instance = u32::try_from(origins.len()).expect("instance count");
                origins.push(origin.extend(0.0).to_array());
                self.draws.push((id, instance, skirts));
                self.triangles += chunk.surface as usize / 3;
            }
            // Only chunks within the shadow range cast: a coarse chunk far away overlaps every
            // cascade sideways, but cannot shade the few hundred metres they cover.
            if let Some((cascades, basis)) = shadows
                && center.length() - chunk.radius < cascades[CASCADES - 1].far
            {
                for (c, cascade) in cascades.iter().enumerate() {
                    if cascade.may_cast_into(basis, center, chunk.radius) {
                        per_cascade[c].push((id, skirts, origin.extend(0.0).to_array()));
                    }
                }
            }
        }
        for (c, list) in per_cascade.into_iter().enumerate() {
            for (id, skirts, origin) in list {
                let instance = u32::try_from(origins.len()).expect("instance count");
                origins.push(origin);
                self.shadow_draws[c].push((id, instance, skirts));
            }
        }
        if origins.len() > self.instance_capacity {
            self.instance_capacity = origins.len().next_power_of_two();
            self.instances = instance_buffer(device, self.instance_capacity);
        }
        queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&origins));
    }

    fn issue(&self, pass: &mut wgpu::RenderPass<'_>, draws: &[(u64, u32, u8)]) {
        pass.set_vertex_buffer(1, self.instances.slice(..));
        for &(id, instance, skirts) in draws {
            let chunk = &self.chunks[&id];
            pass.set_vertex_buffer(0, chunk.vertices.slice(..));
            pass.set_index_buffer(chunk.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..chunk.surface, 0, instance..instance + 1);
            for (face, &(start, end)) in chunk.skirts.iter().enumerate() {
                if skirts & (1 << face) != 0 && end > start {
                    pass.draw_indexed(start..end, 0, instance..instance + 1);
                }
            }
        }
    }

    /// Draws the chunks into the scene pass (globals bound at group 0).
    pub(super) fn draw(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        shadow_sampling: &wgpu::BindGroup,
        luts: &wgpu::BindGroup,
    ) {
        if self.draws.is_empty() {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(1, shadow_sampling, &[]);
        pass.set_bind_group(2, luts, &[]);
        self.issue(pass, &self.draws);
    }

    /// Draws the shadow casters of one cascade (globals and cascade bound by the caller).
    pub(super) fn draw_shadows(&self, pass: &mut wgpu::RenderPass<'_>, cascade: usize) {
        if self.shadow_draws[cascade].is_empty() {
            return;
        }
        pass.set_pipeline(&self.shadow_pipeline);
        self.issue(pass, &self.shadow_draws[cascade]);
    }
}

fn instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("terrain instances"),
        size: (capacity * 16) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

impl Renderer {
    /// Uploads (or replaces) terrain chunk `id`, whose vertices are relative to `origin`.
    pub fn insert_terrain(&mut self, id: u64, origin: DVec3, mesh: &TerrainMeshData) {
        self.terrain.insert(&self.device, id, origin, mesh);
    }

    /// Removes terrain chunk `id` (no-op if absent).
    pub fn remove_terrain(&mut self, id: u64) {
        self.terrain.remove(id);
    }

    /// Number of terrain chunks on the GPU.
    pub fn terrain_chunk_count(&self) -> usize {
        self.terrain.len()
    }
}
