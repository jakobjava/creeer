//! GPU side of the sun shadows: one depth texture array (a layer per cascade), the depth-only
//! pipeline that fills it, and the bind group the mesh shader samples it through.

use super::mesh_pass::{Draw, MeshBuffers, instanced_vertex_layouts};
use crate::mesh::MeshArena;
use crate::shadow::{CASCADES, Cascade};
use std::borrow::Cow;

const SHADOW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

pub(crate) struct ShadowPass {
    pipeline: wgpu::RenderPipeline,
    /// Skinned characters' casters, two-sided (G1.3e).
    two_sided: wgpu::RenderPipeline,
    /// Edge length of each cascade's map in texels.
    size: u32,
    layer_views: Vec<wgpu::TextureView>,
    cascade_buffers: Vec<wgpu::Buffer>,
    cascade_bind_groups: Vec<wgpu::BindGroup>,
    sampler: wgpu::Sampler,
    /// Layout of group 1 in the mesh pipeline (shadow map array + comparison sampler).
    pub sample_layout: wgpu::BindGroupLayout,
    /// Layout of a cascade's matrix (group 1 of shadow-casting pipelines).
    pub cascade_layout: wgpu::BindGroupLayout,
    /// Bind group for group 1 in the mesh pipeline.
    pub sample_bind_group: wgpu::BindGroup,
}

/// The depth texture array: one view per cascade for rendering, and the sampling bind group.
/// Depth state of every shadow-casting pipeline.
pub(super) fn shadow_depth_state() -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: SHADOW_FORMAT,
        depth_write_enabled: Some(true),
        depth_compare: Some(wgpu::CompareFunction::Less),
        stencil: wgpu::StencilState::default(),
        // Slope-scaled bias against self-shadowing on surfaces facing away at an angle.
        bias: wgpu::DepthBiasState {
            constant: 2,
            slope_scale: 2.0,
            clamp: 0.0,
        },
    }
}

fn create_maps(
    device: &wgpu::Device,
    size: u32,
    sample_layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
) -> (Vec<wgpu::TextureView>, wgpu::BindGroup) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("sun shadow cascades"),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: u32::try_from(CASCADES).expect("few cascades"),
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: SHADOW_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let layer_views = (0..CASCADES)
        .map(|layer| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("shadow cascade"),
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: u32::try_from(layer).expect("few cascades"),
                array_layer_count: Some(1),
                ..Default::default()
            })
        })
        .collect();
    let array_view = texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("shadow cascades"),
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("shadow sampling"),
        layout: sample_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&array_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    });
    (layer_views, bind_group)
}

impl ShadowPass {
    pub fn new(
        device: &wgpu::Device,
        globals_layout: &wgpu::BindGroupLayout,
        globals_wgsl: &str,
        size: u32,
    ) -> Self {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow comparison"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            // Lit if the fragment is not further from the sun than the stored occluder.
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let sample_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow sampling"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
            ],
        });
        let (layer_views, sample_bind_group) = create_maps(device, size, &sample_layout, &sampler);

        let cascade_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow cascade"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let cascade_buffers: Vec<wgpu::Buffer> = (0..CASCADES)
            .map(|_| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("shadow cascade"),
                    size: std::mem::size_of::<[f32; 16]>() as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            })
            .collect();
        let cascade_bind_groups = cascade_buffers
            .iter()
            .map(|buffer| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("shadow cascade"),
                    layout: &cascade_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: buffer.as_entire_binding(),
                    }],
                })
            })
            .collect();

        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shadow"),
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(format!(
                "{globals_wgsl}\n{}",
                include_str!("../shaders/shadow.wgsl")
            ))),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shadow"),
            bind_group_layouts: &[Some(globals_layout), Some(&cascade_layout)],
            immediate_size: 0,
        });
        let (plain, skinned) = (
            instanced_vertex_layouts(),
            super::skin_layout::skinned_vertex_layouts(),
        );
        let pipeline = |cull_mode, buffers: &[_]| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("shadow"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers,
                },
                primitive: wgpu::PrimitiveState {
                    cull_mode,
                    ..Default::default()
                },
                depth_stencil: Some(shadow_depth_state()),
                multisample: wgpu::MultisampleState::default(),
                fragment: None,
                multiview_mask: None,
                cache: None,
            })
        };

        Self {
            pipeline: pipeline(Some(wgpu::Face::Back), &plain),
            two_sided: pipeline(None, &skinned),
            size,
            sampler,
            layer_views,
            cascade_buffers,
            cascade_bind_groups,
            sample_layout,
            sample_bind_group,
            cascade_layout,
        }
    }

    /// Edge length of each cascade's map in texels.
    pub fn size(&self) -> u32 {
        self.size
    }

    /// Recreates the maps at a new resolution (no-op if unchanged).
    pub fn resize(&mut self, device: &wgpu::Device, size: u32) {
        if size == self.size {
            return;
        }
        let (views, bind_group) = create_maps(device, size, &self.sample_layout, &self.sampler);
        self.layer_views = views;
        self.sample_bind_group = bind_group;
        self.size = size;
    }

    /// Uploads this frame's cascade matrices.
    pub fn write_cascades(&self, queue: &wgpu::Queue, cascades: &[Cascade; CASCADES]) {
        for (buffer, cascade) in self.cascade_buffers.iter().zip(cascades) {
            queue.write_buffer(
                buffer,
                0,
                bytemuck::cast_slice(&cascade.view_proj.to_cols_array()),
            );
        }
    }

    /// Records one depth pass per cascade.
    #[allow(clippy::too_many_arguments)]
    pub fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        globals: &wgpu::BindGroup,
        buffers: Option<&MeshBuffers<'_>>,
        meshes: &MeshArena,
        draws: &[Vec<Draw>; CASCADES],
        terrain: &super::terrain_pass::TerrainPass,
        skin: &super::skin_pass::SkinPass,
        timer: Option<&super::timing::GpuTimer>,
    ) {
        const NAMES: [&str; CASCADES] = ["shadow 0", "shadow 1", "shadow 2", "shadow 3"];
        for (cascade, view) in self.layer_views.iter().enumerate() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow cascade"),
                timestamp_writes: timer.and_then(|t| t.render(NAMES[cascade])),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, globals, &[]);
            pass.set_bind_group(1, &self.cascade_bind_groups[cascade], &[]);
            if let Some(buffers) = buffers {
                buffers.draw(&mut pass, meshes, &draws[cascade]);
            }
            terrain.draw_shadows(&mut pass, cascade);
            if !skin.shadow_draws[cascade].is_empty() {
                pass.set_pipeline(&self.two_sided);
                skin.draw(&mut pass, &skin.shadow_draws[cascade]);
            }
        }
    }
}
