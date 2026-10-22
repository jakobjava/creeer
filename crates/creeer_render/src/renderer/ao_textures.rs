//! Ambient occlusion's textures and bind groups, made for one screen size.

/// MIPs of the view-space depth chain (XeGTAO's `XE_GTAO_DEPTH_MIP_LEVELS`).
const DEPTH_MIPS: u32 = 5;

/// The compute passes' bind group layouts.
pub(super) struct Layouts {
    pub prefilter: wgpu::BindGroupLayout,
    pub last_mip: wgpu::BindGroupLayout,
    pub main: wgpu::BindGroupLayout,
    pub denoise: wgpu::BindGroupLayout,
}

fn entry(binding: u32, ty: wgpu::BindingType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty,
        count: None,
    }
}

fn uniform() -> wgpu::BindingType {
    wgpu::BindingType::Buffer {
        ty: wgpu::BufferBindingType::Uniform,
        has_dynamic_offset: false,
        min_binding_size: None,
    }
}

fn storage(format: wgpu::TextureFormat) -> wgpu::BindingType {
    wgpu::BindingType::StorageTexture {
        access: wgpu::StorageTextureAccess::WriteOnly,
        format,
        view_dimension: wgpu::TextureViewDimension::D2,
    }
}

fn unfiltered() -> wgpu::BindingType {
    wgpu::BindingType::Texture {
        sample_type: wgpu::TextureSampleType::Float { filterable: false },
        view_dimension: wgpu::TextureViewDimension::D2,
        multisampled: false,
    }
}

impl Layouts {
    pub fn new(device: &wgpu::Device) -> Self {
        let layout = |label, entries: &[wgpu::BindGroupLayoutEntry]| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries,
            })
        };
        let r32 = wgpu::TextureFormat::R32Float;
        let rgba8 = wgpu::TextureFormat::Rgba8Unorm;
        let mut prefilter = vec![
            entry(0, uniform()),
            entry(
                1,
                wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: true,
                },
            ),
        ];
        prefilter.extend((0..DEPTH_MIPS - 1).map(|m| entry(2 + m, storage(r32))));
        Self {
            prefilter: layout("ao depth", &prefilter),
            last_mip: layout(
                "ao last mip",
                &[
                    entry(0, uniform()),
                    entry(6, unfiltered()),
                    entry(7, storage(r32)),
                ],
            ),
            main: layout(
                "gtao",
                &[
                    entry(0, uniform()),
                    entry(1, unfiltered()),
                    entry(
                        2,
                        wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    ),
                    entry(3, storage(r32)),
                ],
            ),
            denoise: layout(
                "ao denoise",
                &[
                    entry(0, uniform()),
                    entry(1, unfiltered()),
                    entry(2, unfiltered()),
                    entry(3, storage(rgba8)),
                ],
            ),
        }
    }
}

/// What the scene's shaders bind to read the occlusion (group 3).
pub(super) fn sample_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    let fragment = |binding, ty| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty,
        count: None,
    };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("ao sample"),
        entries: &[
            fragment(
                0,
                wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
            ),
            fragment(
                1,
                wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            ),
            fragment(2, uniform()),
        ],
    })
}

/// The textures for one screen size and resolution.
pub(super) struct AoTextures {
    /// The screen's size, and the grid occlusion is computed on.
    pub full: (u32, u32),
    pub grid: (u32, u32),
    /// The last pass's output, which the scene reads.
    pub result: wgpu::Texture,
    pub prefilter: wgpu::BindGroup,
    pub last_mip: wgpu::BindGroup,
    pub main: wgpu::BindGroup,
    pub denoise: wgpu::BindGroup,
    result_view: wgpu::TextureView,
    sampler: wgpu::Sampler,
}

impl AoTextures {
    pub fn new(
        device: &wgpu::Device,
        layouts: &Layouts,
        constants: &wgpu::Buffer,
        depth: &wgpu::TextureView,
        full: (u32, u32),
        base_mip: u32,
    ) -> Self {
        let grid = ((full.0 >> base_mip).max(1), (full.1 >> base_mip).max(1));
        let texture = |label, (width, height): (u32, u32), format, mips, extra| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: mips,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::STORAGE_BINDING
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | extra,
                view_formats: &[],
            })
        };
        let r32 = wgpu::TextureFormat::R32Float;
        let rgba8 = wgpu::TextureFormat::Rgba8Unorm;
        let none = wgpu::TextureUsages::empty();
        // Five levels need 16 pixels each way; smaller screens compute on a padded chain.
        let padded = (full.0.max(16), full.1.max(16));
        let mips = texture("ao depth mips", padded, r32, DEPTH_MIPS, none);
        let mip_view = |m| {
            mips.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: m,
                mip_level_count: Some(1),
                ..Default::default()
            })
        };
        let mip_views: Vec<_> = (0..DEPTH_MIPS).map(mip_view).collect();
        let view = |t: &wgpu::Texture| t.create_view(&wgpu::TextureViewDescriptor::default());
        let a = view(&texture("ao a", grid, r32, 1, none));
        let result = texture("ao", grid, rgba8, 1, wgpu::TextureUsages::COPY_SRC);
        let result_view = view(&result);
        let group = |label, layout, resources: &[wgpu::BindingResource<'_>]| {
            let entries: Vec<_> = resources
                .iter()
                .enumerate()
                .map(|(i, resource)| wgpu::BindGroupEntry {
                    binding: u32::try_from(i).expect("few bindings"),
                    resource: resource.clone(),
                })
                .collect();
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout,
                entries: &entries,
            })
        };
        let tv = wgpu::BindingResource::TextureView;
        let mut prefilter = vec![constants.as_entire_binding(), tv(depth)];
        prefilter.extend(mip_views[..DEPTH_MIPS as usize - 1].iter().map(tv));
        let last_mip = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ao last mip"),
            layout: &layouts.last_mip,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: constants.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: tv(&mip_views[3]),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: tv(&mip_views[4]),
                },
            ],
        });
        let nearest = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ao nearest"),
            ..Default::default()
        });
        let all_mips = view(&mips);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ao linear"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            full,
            grid,
            prefilter: group("ao depth", &layouts.prefilter, &prefilter),
            last_mip,
            main: group(
                "gtao",
                &layouts.main,
                &[
                    constants.as_entire_binding(),
                    tv(&all_mips),
                    wgpu::BindingResource::Sampler(&nearest),
                    tv(&a),
                ],
            ),
            denoise: group(
                "ao denoise",
                &layouts.denoise,
                &[
                    constants.as_entire_binding(),
                    tv(&a),
                    tv(&all_mips),
                    tv(&result_view),
                ],
            ),
            result,
            result_view,
            sampler,
        }
    }

    /// The bind group the scene reads the occlusion through.
    pub fn sample_bind_group(
        &self,
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        view: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ao sample"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&self.result_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: view.as_entire_binding(),
                },
            ],
        })
    }
}
