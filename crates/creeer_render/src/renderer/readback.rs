//! Reading rendered frames back from the GPU (screenshots, image tests).

use super::*;

impl Renderer {
    /// Draws one frame into an image (works with or without a window).
    ///
    /// # Panics
    /// If the GPU readback fails (device lost).
    pub fn capture(&mut self, mut frame: Frame<'_>) -> Image {
        self.apply_ui_textures(&mut frame);
        let (width, height) = self.size;
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("capture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.output_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let padded_row = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("capture readback"),
            // The image, followed by the 16-byte exposure state.
            size: u64::from(padded_row * height) + 16,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let image_bytes = u64::from(padded_row * height);
        let exposure = self.exposure.exposure.clone();
        let copy = |encoder: &mut wgpu::CommandEncoder| {
            encoder.copy_buffer_to_buffer(&exposure, 0, &readback, image_bytes, 16);
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &readback,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(padded_row),
                        rows_per_image: Some(height),
                    },
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        };
        let commands = self.encode(&frame, &view, Some(&copy), true);
        self.queue.submit(commands);
        self.free_ui_textures(&mut frame);

        let (sender, receiver) = std::sync::mpsc::channel();
        readback.map_async(wgpu::MapMode::Read, .., move |result| {
            let _ = sender.send(result);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("GPU poll failed");
        receiver
            .recv()
            .expect("map callback dropped")
            .expect("GPU readback failed");

        let mapped = readback.get_mapped_range(..).expect("readback not mapped");
        let state = |i: usize| {
            let at = usize::try_from(image_bytes).expect("fits") + 4 * i;
            f32::from_le_bytes(mapped[at..at + 4].try_into().expect("4 bytes"))
        };
        self.stats.exposure = state(0);
        self.stats.average_luminance = state(1);
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for row in mapped[..usize::try_from(image_bytes).expect("fits")]
            .chunks(padded_row as usize)
            .take(height as usize)
        {
            rgba.extend_from_slice(&row[..(width * 4) as usize]);
        }
        drop(mapped);
        readback.unmap();
        if matches!(
            self.output_format,
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        ) {
            for pixel in rgba.as_chunks_mut::<4>().0 {
                pixel.swap(0, 2);
            }
        }
        Image {
            width,
            height,
            rgba,
        }
    }
}
