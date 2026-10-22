//! Render targets and pipeline state shared by the scene passes.

use super::*;

pub(super) fn shader(device: &wgpu::Device, label: &str, body: &str) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(Cow::Owned(format!("{GLOBALS_WGSL}\n{body}"))),
    })
}

/// Off-screen render targets, recreated on resize.
pub(super) struct Targets {
    pub(super) hdr_msaa: wgpu::TextureView,
    pub(super) hdr_resolved: wgpu::TextureView,
    pub(super) depth: wgpu::TextureView,
}

impl Targets {
    pub(super) fn new(device: &wgpu::Device, (width, height): (u32, u32)) -> Self {
        let make = |label, format, samples, usage| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        Self {
            hdr_msaa: make(
                "hdr msaa",
                HDR_FORMAT,
                MSAA_SAMPLES,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
            ),
            hdr_resolved: make(
                "hdr resolved",
                HDR_FORMAT,
                1,
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            ),
            // Read by ambient occlusion between the prepass and the scene pass.
            depth: make(
                "depth",
                DEPTH_FORMAT,
                MSAA_SAMPLES,
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            ),
        }
    }
}

pub(super) fn scene_depth_state() -> Option<wgpu::DepthStencilState> {
    Some(wgpu::DepthStencilState {
        format: DEPTH_FORMAT,
        depth_write_enabled: Some(true),
        // Reversed-Z: nearer fragments have *greater* depth.
        depth_compare: Some(wgpu::CompareFunction::Greater),
        stencil: wgpu::StencilState::default(),
        bias: wgpu::DepthBiasState::default(),
    })
}

/// What the scene pass's meshes and terrain test against the depth their prepass wrote: the
/// same depth, from the same vertex shader (`@invariant`), so each pixel is shaded once.
pub(super) fn after_prepass_depth_state() -> Option<wgpu::DepthStencilState> {
    Some(wgpu::DepthStencilState {
        depth_write_enabled: Some(false),
        depth_compare: Some(wgpu::CompareFunction::Equal),
        ..scene_depth_state()?
    })
}

/// A depth-only pipeline for the prepass (G1.3d): `entry` of `module`, whose vertex stage reads
/// the globals (group 0) only.
pub(super) fn prepass_pipeline(
    device: &wgpu::Device,
    label: &str,
    globals_layout: &wgpu::BindGroupLayout,
    module: &wgpu::ShaderModule,
    entry: &str,
    buffers: &[Option<wgpu::VertexBufferLayout<'_>>],
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &[Some(globals_layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some(entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers,
        },
        primitive: wgpu::PrimitiveState {
            cull_mode: Some(wgpu::Face::Back),
            ..Default::default()
        },
        depth_stencil: scene_depth_state(),
        multisample: scene_multisample(),
        fragment: None,
        multiview_mask: None,
        cache: None,
    })
}

pub(super) fn scene_multisample() -> wgpu::MultisampleState {
    wgpu::MultisampleState {
        count: MSAA_SAMPLES,
        ..Default::default()
    }
}
