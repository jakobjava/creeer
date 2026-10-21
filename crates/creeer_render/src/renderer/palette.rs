//! The palette on the GPU (G1.3b, ADR-0026): one entry per token of `creeer_art::Palette`, in
//! its order, which model vertices name by index (`MeshVertex::material`).

use bytemuck::{Pod, Zeroable};
use creeer_art::{Palette, Pattern, Shade};

/// One token as the mesh shader reads it (48 bytes, `surface.wgsl`'s `Token`).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub(crate) struct TokenRaw {
    /// Linear base colour.
    pub colour: [f32; 3],
    pub roughness: f32,
    pub metalness: f32,
    /// 1 when each player has their own colour (the instance's colour is used).
    pub per_player: f32,
    /// The token wear reveals (its own index when none).
    pub wears_to: u32,
    /// The token age turns it into (its own index when none).
    pub ages_to: u32,
    /// Its pattern (`surface.wgsl`'s `PATTERN_*`).
    pub pattern: u32,
    /// The years ageing takes (0 when it does not age).
    pub weathers_in: f32,
    pub _pad: [u32; 2],
}

/// A pattern's number in the shader.
fn pattern(p: Pattern) -> u32 {
    match p {
        Pattern::Plain => 0,
        Pattern::Wood => 1,
        Pattern::Paint => 2,
        Pattern::Metal => 3,
        Pattern::Stone => 4,
        Pattern::Fabric => 5,
        Pattern::Leather => 6,
        Pattern::Glass => 7,
    }
}

/// The tokens of `palette` for the GPU. Never empty: a storage buffer cannot be.
pub(crate) fn tokens(palette: &Palette) -> Vec<TokenRaw> {
    let mut out: Vec<TokenRaw> = palette
        .tokens
        .iter()
        .enumerate()
        .map(|(k, t)| TokenRaw {
            colour: match t.shade {
                Shade::Fixed(c) => c.linear().map(|x| {
                    #[allow(clippy::cast_possible_truncation)] // colours to the GPU's f32
                    let f = x as f32;
                    f
                }),
                // Not coloured yet, or coloured by the instance: a neutral grey meanwhile.
                Shade::PerPlayer | Shade::FromTemperature | Shade::Unset => [0.5; 3],
            },
            roughness: narrow(t.roughness),
            metalness: narrow(t.metalness),
            per_player: if t.shade == Shade::PerPlayer {
                1.0
            } else {
                0.0
            },
            wears_to: u32::try_from(t.wears_to.unwrap_or(k)).expect("few tokens"),
            ages_to: u32::try_from(t.ages_to.unwrap_or(k)).expect("few tokens"),
            pattern: pattern(t.pattern),
            weathers_in: t.weathers_in.map_or(0.0, narrow),
            _pad: [0; 2],
        })
        .collect();
    if out.is_empty() {
        out.push(TokenRaw::zeroed());
    }
    out
}

fn narrow(x: f64) -> f32 {
    #[allow(clippy::cast_possible_truncation)] // 0–1
    let f = x as f32;
    f
}

/// What the surfaces read (group 2 of the mesh pipelines): the palette's buffer, and the
/// noise volume (`noise.rs`) with its sampler.
pub(crate) struct GpuPalette {
    pub layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    buffer: wgpu::Buffer,
    noise: (wgpu::TextureView, wgpu::Sampler),
}

impl GpuPalette {
    /// An empty palette (until one is set, model vertices draw neutral grey), and the noise.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let fragment = wgpu::ShaderStages::FRAGMENT;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("palette"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: fragment,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: fragment,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D3,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: fragment,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let noise = super::noise::upload(device, queue);
        let (buffer, bind_group) =
            Self::make(device, &layout, &noise, &tokens(&Palette::default()));
        Self {
            layout,
            bind_group,
            buffer,
            noise,
        }
    }

    fn make(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        (view, sampler): &(wgpu::TextureView, wgpu::Sampler),
        tokens: &[TokenRaw],
    ) -> (wgpu::Buffer, wgpu::BindGroup) {
        let buffer = wgpu::util::DeviceExt::create_buffer_init(
            device,
            &wgpu::util::BufferInitDescriptor {
                label: Some("palette"),
                contents: bytemuck::cast_slice(tokens),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            },
        );
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("palette"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });
        (buffer, bind_group)
    }

    /// Replaces the tokens with `palette`'s.
    pub fn set(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, palette: &Palette) {
        let tokens = tokens(palette);
        if std::mem::size_of_val(tokens.as_slice()) as u64 == self.buffer.size() {
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&tokens));
        } else {
            (self.buffer, self.bind_group) = Self::make(device, &self.layout, &self.noise, &tokens);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use creeer_data::Data;

    #[test]
    fn tokens_reach_the_gpu_in_order_with_their_wear() {
        let palette = Palette::from_data(&Data::base()).expect("the base palette");
        let raw = tokens(&palette);
        assert_eq!(raw.len(), palette.tokens.len());
        assert_eq!(std::mem::size_of::<TokenRaw>(), 48);
        let timber = palette.index("timber").expect("timber");
        assert_eq!(raw[timber].pattern, 1);
        assert_eq!(
            raw[timber].ages_to as usize,
            palette.index("timber_weathered").expect("aged")
        );
        assert!((raw[timber].weathers_in - 1.0).abs() < 1e-6);
        let k = palette.index("steel_painted").expect("painted steel");
        let steel = palette.index("steel").expect("steel");
        assert_eq!(raw[k].wears_to as usize, steel);
        assert!((raw[steel].metalness - 1.0).abs() < 1e-6);
        // sRGB #6f7a6c as linear light.
        let [r, g, b] = raw[k].colour;
        assert!((r - 0.159).abs() < 1e-3 && (g - 0.195).abs() < 1e-3 && (b - 0.150).abs() < 1e-3);
        let accent = palette.index("player_accent").expect("the accent");
        assert!((raw[accent].per_player - 1.0).abs() < 1e-6);
        assert_eq!(tokens(&Palette::default()).len(), 1);
    }
}
