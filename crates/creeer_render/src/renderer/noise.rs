//! The noise volume the surfaces sample (G1.3c, `surface.wgsl`): 32³ random values, made from
//! a fixed hash (so every machine has the same), tiling, filtered linearly.
//!
//! A fetch with its fraction warped by a quintic (Quilez's improved texture interpolation)
//! is a smooth value noise in one texture lookup. Computing it costs eight hashes and seven
//! interpolations per octave: Unreal's noise takes about 16 instructions and one lookup per
//! level from a 3D texture against 53 for its computed value noise. Measured here: the
//! computed noise cost about 0.14 ms of a 1080p frame of 10,000 parts (ADR-0026).

/// The volume's size per side, in cells: the period of each octave.
pub(crate) const SIZE: u32 = 32;

/// The value of cell (x, y, z): a fixed hash of its coordinates, as a byte.
fn cell(x: u32, y: u32, z: u32) -> u8 {
    let mut h =
        x.wrapping_mul(0x8da6_b343) ^ y.wrapping_mul(0xd816_3841) ^ z.wrapping_mul(0xcb1a_b31f);
    h = (h ^ (h >> 16)).wrapping_mul(0x7feb_352d);
    h = (h ^ (h >> 15)).wrapping_mul(0x846c_a68b);
    h ^= h >> 16;
    h.to_le_bytes()[3]
}

/// The volume's bytes, x fastest.
pub(crate) fn values() -> Vec<u8> {
    let mut out = Vec::with_capacity((SIZE * SIZE * SIZE) as usize);
    for z in 0..SIZE {
        for y in 0..SIZE {
            for x in 0..SIZE {
                out.push(cell(x, y, z));
            }
        }
    }
    out
}

/// The volume on the GPU, and a sampler that tiles it and filters linearly.
pub(crate) fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> (wgpu::TextureView, wgpu::Sampler) {
    let texture = wgpu::util::DeviceExt::create_texture_with_data(
        device,
        queue,
        &wgpu::TextureDescriptor {
            label: Some("noise volume"),
            size: wgpu::Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: SIZE,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        &values(),
    );
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("noise volume"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        address_mode_w: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    (
        texture.create_view(&wgpu::TextureViewDescriptor::default()),
        sampler,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_volume_is_fixed_and_evenly_spread() {
        let v = values();
        assert_eq!(v.len(), 32 * 32 * 32);
        assert_eq!(v, values(), "the same every time");
        // A fair spread over the bytes: mean near the middle, every quarter well used.
        let mean = v.iter().map(|&b| f64::from(b)).sum::<f64>() / v.len() as f64;
        assert!((mean - 127.5).abs() < 2.0, "{mean}");
        for quarter in 0..4_u8 {
            let share = v.iter().filter(|&&b| b / 64 == quarter).count() as f64 / v.len() as f64;
            assert!((share - 0.25).abs() < 0.01, "{share}");
        }
    }
}
