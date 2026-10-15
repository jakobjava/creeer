//! Sky: atmosphere, planet surface, sun and stars.

use super::*;

/// Atmosphere, planet surface, sun and stars behind all geometry.
pub(super) struct SkyPass {
    pipeline: wgpu::RenderPipeline,
    transmittance: wgpu::Texture,
    multiscatter: wgpu::Texture,
    luts_bind_group: wgpu::BindGroup,
    /// Atmosphere whose tables are currently uploaded.
    uploaded: Option<Atmosphere>,
}

impl SkyPass {
    pub(super) fn new(device: &wgpu::Device, globals_layout: &wgpu::BindGroupLayout) -> Self {
        let module = shader(device, "sky", include_str!("../shaders/sky.wgsl"));
        let lut_texture = |label, (width, height): (usize, usize)| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: u32::try_from(width).expect("small table"),
                    height: u32::try_from(height).expect("small table"),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        };
        let transmittance = lut_texture("atmosphere transmittance", TRANSMITTANCE_LUT);
        let multiscatter = lut_texture(
            "atmosphere multiple scattering",
            (MULTISCATTER_LUT, MULTISCATTER_LUT),
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("atmosphere tables"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let texture_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let luts_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("atmosphere tables"),
            entries: &[
                texture_entry(0),
                texture_entry(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let luts_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("atmosphere tables"),
            layout: &luts_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &transmittance.create_view(&wgpu::TextureViewDescriptor::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(
                        &multiscatter.create_view(&wgpu::TextureViewDescriptor::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sky"),
            bind_group_layouts: &[Some(globals_layout), Some(&luts_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sky"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                // The sky triangle sits at depth 0 (infinitely far): it passes only where
                // the depth buffer still holds its cleared value.
                depth_compare: Some(wgpu::CompareFunction::Equal),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
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
        Self {
            pipeline,
            transmittance,
            multiscatter,
            luts_bind_group,
            uploaded: None,
        }
    }

    /// Uploads the precomputed tables if the atmosphere changed.
    pub(super) fn prepare(&mut self, queue: &wgpu::Queue, atmosphere: &Atmosphere) {
        if self.uploaded.as_ref() == Some(atmosphere) {
            return;
        }
        let luts = atmosphere_luts(atmosphere);
        let upload =
            |texture: &wgpu::Texture, data: &[[f32; 3]], (width, height): (usize, usize)| {
                let texels: Vec<u16> = data
                    .iter()
                    .flat_map(|&[r, g, b]| [r, g, b, 1.0].map(f16_bits))
                    .collect();
                let (width, height) = (
                    u32::try_from(width).expect("small table"),
                    u32::try_from(height).expect("small table"),
                );
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    bytemuck::cast_slice(&texels),
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(width * 8),
                        rows_per_image: Some(height),
                    },
                    wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                );
            };
        upload(&self.transmittance, &luts.transmittance, TRANSMITTANCE_LUT);
        upload(
            &self.multiscatter,
            &luts.multiscatter,
            (MULTISCATTER_LUT, MULTISCATTER_LUT),
        );
        self.uploaded = Some(*atmosphere);
    }

    pub(super) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(1, &self.luts_bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}
