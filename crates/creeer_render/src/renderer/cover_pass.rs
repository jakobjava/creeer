//! Decoratives on the ground (G2.11, ADR-0037; `cover.rs`, `cover.wgsl`): one instanced draw a
//! shape, in the depth prepass (so they take ambient occlusion and give the ground their contact
//! shadows) and in the scene pass, drawn with the terrain. The client hands the instances over
//! each frame, camera-relative and already culled (`Renderer::set_cover`).

use super::*;
use crate::cover::{CoverInstance, CoverVertex, pebble, tuft};
use wgpu::util::DeviceExt;

const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32];
const INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
    3 => Float32x3, 4 => Float32x2, 5 => Snorm8x4, 6 => Unorm8x4, 7 => Float32
];

fn layouts() -> [Option<wgpu::VertexBufferLayout<'static>>; 2] {
    [
        Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<CoverVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &VERTEX_ATTRIBUTES,
        }),
        Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<CoverInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &INSTANCE_ATTRIBUTES,
        }),
    ]
}

/// The decoratives' pipelines, shapes and this frame's instances.
pub(super) struct CoverPass {
    /// Drawn alone, and after the depth prepass.
    pipeline: [wgpu::RenderPipeline; 2],
    prepass: wgpu::RenderPipeline,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    /// Each shape's indices (first, count) and base vertex, in `CoverShape` order.
    shapes: [(u32, u32, i32); 2],
    instances: wgpu::Buffer,
    capacity: usize,
    /// This frame's instances of each shape, one after the other.
    counts: [u32; 2],
}

impl CoverPass {
    /// `layouts`: globals, shadow sampling, the atmosphere's tables, ambient occlusion.
    pub(super) fn new(device: &wgpu::Device, groups: [&wgpu::BindGroupLayout; 4]) -> Self {
        let source = concat!(
            include_str!("../shaders/ao_sample.wgsl"),
            "\n",
            include_str!("../shaders/lighting.wgsl"),
            "\n",
            include_str!("../shaders/cover.wgsl")
        );
        let module = shader(device, "cover", source);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cover"),
            bind_group_layouts: &groups.map(Some),
            immediate_size: 0,
        });
        let pipeline = |depth_stencil| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("cover"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &layouts(),
                },
                // Blades are seen from both sides.
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil,
                multisample: scene_multisample(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(HDR_FORMAT.into())],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let prepass = prepass_pipeline(
            device,
            "cover prepass",
            groups[0],
            &module,
            "vs_main",
            &layouts(),
            None,
        );
        let (mut vertices, mut indices) = tuft();
        let tuft_indices = u32::try_from(indices.len()).expect("small mesh");
        let base = i32::try_from(vertices.len()).expect("small mesh");
        let (pebble_vertices, pebble_indices) = pebble();
        let pebble_count = u32::try_from(pebble_indices.len()).expect("small mesh");
        vertices.extend(pebble_vertices);
        indices.extend(pebble_indices);
        let buffer = |label, contents: &[u8], usage| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents,
                usage,
            })
        };
        Self {
            pipeline: [scene_depth_state(), after_prepass_depth_state()].map(pipeline),
            prepass,
            vertices: buffer(
                "cover vertices",
                bytemuck::cast_slice(&vertices),
                wgpu::BufferUsages::VERTEX,
            ),
            indices: buffer(
                "cover indices",
                bytemuck::cast_slice(&indices),
                wgpu::BufferUsages::INDEX,
            ),
            shapes: [(0, tuft_indices, 0), (tuft_indices, pebble_count, base)],
            instances: instance_buffer(device, 1024),
            capacity: 1024,
            counts: [0; 2],
        }
    }

    /// This frame's instances: each shape's, in `CoverShape` order.
    pub(super) fn set(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        shapes: [&[CoverInstance]; 2],
    ) {
        let total = shapes[0].len() + shapes[1].len();
        if total > self.capacity {
            self.capacity = total.next_power_of_two();
            self.instances = instance_buffer(device, self.capacity);
        }
        let mut at = 0;
        for (k, list) in shapes.iter().enumerate() {
            queue.write_buffer(&self.instances, at, bytemuck::cast_slice(list));
            at += std::mem::size_of_val(*list) as u64;
            self.counts[k] = u32::try_from(list.len()).expect("instance count");
        }
    }

    fn issue(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_vertex_buffer(1, self.instances.slice(..));
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        let mut first = 0;
        for (&(start, count, base), &n) in self.shapes.iter().zip(&self.counts) {
            if n > 0 {
                pass.draw_indexed(start..start + count, base, first..first + n);
            }
            first += n;
        }
    }

    /// Draws the decoratives into the scene pass (globals bound at group 0).
    pub(super) fn draw(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        groups: [&wgpu::BindGroup; 3],
        prepassed: bool,
    ) {
        if self.counts == [0; 2] {
            return;
        }
        pass.set_pipeline(&self.pipeline[usize::from(prepassed)]);
        for (k, group) in groups.into_iter().enumerate() {
            pass.set_bind_group(u32::try_from(k + 1).expect("four groups"), group, &[]);
        }
        self.issue(pass);
    }

    /// Draws them into the depth prepass (globals bound by the caller).
    pub(super) fn draw_prepass(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.counts != [0; 2] {
            pass.set_pipeline(&self.prepass);
            self.issue(pass);
        }
    }
}

fn instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("cover instances"),
        size: (capacity * std::mem::size_of::<CoverInstance>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
