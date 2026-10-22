//! Automatic exposure: histogram of the HDR image and smooth adaptation (exposure.wgsl).

use super::*;

/// Histogram bins (must match exposure.wgsl).
const BINS: u64 = 256;
/// Side of the square workgroup of the histogram pass (must match exposure.wgsl).
const TILE: u32 = 16;

pub(super) struct ExposurePass {
    histogram_pipeline: wgpu::ComputePipeline,
    adapt_pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    histogram: wgpu::Buffer,
    /// x: current exposure, y: average luminance, z: target exposure. Read by tonemapping.
    pub(super) exposure: wgpu::Buffer,
    /// A fixed exposure instead of the automatic one ([`Renderer::fix_exposure`]).
    fixed: Option<f32>,
}

impl ExposurePass {
    pub(super) fn new(
        device: &wgpu::Device,
        globals_layout: &wgpu::BindGroupLayout,
        targets: &Targets,
    ) -> Self {
        let storage = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: false },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("exposure"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                storage(1),
                storage(2),
            ],
        });
        let histogram = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("luminance histogram"),
            size: BINS * 4,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        // Starts at zero, which the shader treats as "no history yet".
        let exposure = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("exposure"),
            size: 16,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let module = shader(device, "exposure", include_str!("../shaders/exposure.wgsl"));
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("exposure"),
            bind_group_layouts: &[Some(globals_layout), Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some(entry),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            })
        };
        let bind_group = Self::bind(device, &layout, targets, &histogram, &exposure);
        Self {
            histogram_pipeline: pipeline("histogram_main"),
            adapt_pipeline: pipeline("adapt_main"),
            layout,
            bind_group,
            histogram,
            exposure,
            fixed: None,
        }
    }

    fn bind(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        targets: &Targets,
        histogram: &wgpu::Buffer,
        exposure: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("exposure"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&targets.hdr_resolved),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: histogram.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: exposure.as_entire_binding(),
                },
            ],
        })
    }

    /// Call after the render targets were recreated.
    pub(super) fn rebind(&mut self, device: &wgpu::Device, targets: &Targets) {
        self.bind_group = Self::bind(
            device,
            &self.layout,
            targets,
            &self.histogram,
            &self.exposure,
        );
    }

    /// Measures the resolved HDR image and updates the exposure buffer, or writes the fixed
    /// exposure into it.
    pub(super) fn encode(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        globals: &wgpu::BindGroup,
        (width, height): (u32, u32),
        timer: Option<&super::timing::GpuTimer>,
    ) {
        if let Some(e) = self.fixed {
            queue.write_buffer(&self.exposure, 0, bytemuck::cast_slice(&[e, 0.0, e, 0.0]));
            return;
        }
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("exposure"),
            timestamp_writes: timer.and_then(|t| t.compute("exposure")),
        });
        pass.set_bind_group(0, globals, &[]);
        pass.set_bind_group(1, &self.bind_group, &[]);
        pass.set_pipeline(&self.histogram_pipeline);
        pass.dispatch_workgroups(width.div_ceil(TILE), height.div_ceil(TILE), 1);
        pass.set_pipeline(&self.adapt_pipeline);
        pass.dispatch_workgroups(1, 1, 1);
    }
}

impl Renderer {
    /// Draws at exposure `fixed` from now on instead of adapting to the image, or adapts again
    /// with `None`: images that must compare by their pixels, such as an icon on two
    /// backgrounds (`creeer_client` icons), are exposed alike.
    pub fn fix_exposure(&mut self, fixed: Option<f32>) {
        self.exposure.fixed = fixed;
    }
}
