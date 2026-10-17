//! The sea: a planet's ocean at sea level, drawn per pixel where the view meets it (`sea.wgsl`).

use super::*;

/// The sea's pipeline (its atmosphere tables are the sky pass's).
pub(super) struct SeaPass {
    pipeline: wgpu::RenderPipeline,
}

impl SeaPass {
    pub(super) fn new(
        device: &wgpu::Device,
        globals_layout: &wgpu::BindGroupLayout,
        luts_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let source = concat!(
            include_str!("../shaders/atmosphere.wgsl"),
            "\n",
            include_str!("../shaders/sea.wgsl")
        );
        let module = shader(device, "sea", source);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sea"),
            bind_group_layouts: &[Some(globals_layout), Some(luts_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sea"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            // Its depth is the sea's own (written by the shader): land nearer hides it.
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
        Self { pipeline }
    }

    /// Draws it (after the land and the meshes, before the sky).
    pub(super) fn draw(&self, pass: &mut wgpu::RenderPass<'_>, luts: &wgpu::BindGroup) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(1, luts, &[]);
        pass.draw(0..3, 0..1);
    }
}
