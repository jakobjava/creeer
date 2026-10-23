//! Rendered images read back from the GPU, e.g. for screenshots and image tests.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

/// An 8-bit RGBA image, rows top to bottom.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width × height × 4` bytes, sRGB-encoded RGBA.
    pub rgba: Vec<u8>,
}

impl Image {
    /// RGBA of the pixel at (x, y).
    ///
    /// # Panics
    /// If the coordinates are outside the image.
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        assert!(x < self.width && y < self.height, "pixel out of bounds");
        let i = ((y * self.width + x) * 4) as usize;
        [
            self.rgba[i],
            self.rgba[i + 1],
            self.rgba[i + 2],
            self.rgba[i + 3],
        ]
    }

    /// Writes the image as a PNG file.
    ///
    /// # Errors
    /// If the file cannot be created or written.
    pub fn save_png(&self, path: &Path) -> std::io::Result<()> {
        let file = BufWriter::new(File::create(path)?);
        let mut encoder = png::Encoder::new(file, self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(std::io::Error::other)?;
        writer
            .write_image_data(&self.rgba)
            .map_err(std::io::Error::other)?;
        writer.finish().map_err(std::io::Error::other)
    }
}

impl Image {
    /// Reads an 8-bit RGBA or RGB PNG file (golden images, G1.3h).
    ///
    /// # Errors
    /// If the file cannot be read or is not such a PNG.
    pub fn load_png(path: &Path) -> std::io::Result<Self> {
        let decoder = png::Decoder::new(std::io::BufReader::new(File::open(path)?));
        let mut reader = decoder.read_info().map_err(std::io::Error::other)?;
        let mut buffer = vec![0; reader.output_buffer_size().unwrap_or(0)];
        let info = reader
            .next_frame(&mut buffer)
            .map_err(std::io::Error::other)?;
        if info.bit_depth != png::BitDepth::Eight {
            return Err(std::io::Error::other("not an 8-bit PNG"));
        }
        let pixels = &buffer[..info.buffer_size()];
        let rgba = match info.color_type {
            png::ColorType::Rgba => pixels.to_vec(),
            png::ColorType::Rgb => pixels
                .chunks(3)
                .flat_map(|p| [p[0], p[1], p[2], 255])
                .collect(),
            other => return Err(std::io::Error::other(format!("a PNG of {other:?}"))),
        };
        Ok(Self {
            width: info.width,
            height: info.height,
            rgba,
        })
    }
}
