//! HDR to display conversion.

use super::*;

/// HDR → output conversion.
pub(super) struct TonemapPass {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    /// Current exposure from the exposure pass (read in the fragment shader).
    exposure: wgpu::Buffer,
}

impl TonemapPass {
    pub(super) fn new(
        device: &wgpu::Device,
        globals_layout: &wgpu::BindGroupLayout,
        output_format: wgpu::TextureFormat,
        targets: &Targets,
        exposure: &wgpu::Buffer,
    ) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tonemap input"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let module = shader(device, "tonemap", include_str!("../shaders/tonemap.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("tonemap"),
            bind_group_layouts: &[Some(globals_layout), Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("tonemap"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(output_format.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
        let bind_group = Self::bind(device, &layout, targets, exposure);
        Self {
            pipeline,
            layout,
            bind_group,
            exposure: exposure.clone(),
        }
    }

    pub(super) fn bind(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        targets: &Targets,
        exposure: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tonemap input"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&targets.hdr_resolved),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: exposure.as_entire_binding(),
                },
            ],
        })
    }

    pub(super) fn rebind(&mut self, device: &wgpu::Device, targets: &Targets) {
        self.bind_group = Self::bind(device, &self.layout, targets, &self.exposure);
    }

    pub(super) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(1, &self.bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}
