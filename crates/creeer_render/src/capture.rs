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
