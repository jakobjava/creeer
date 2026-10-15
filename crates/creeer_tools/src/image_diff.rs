//! `creeer_tools image-diff <a.png> <b.png>`: how different are two rendered images?
//!
//! Used to quantify temporal artefacts: render two consecutive frames and count the pixels
//! that change noticeably. Shadow flicker shows up as a higher changed-pixel share.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::process::ExitCode;

/// A channel difference above this (0–255) counts as a visible change.
const VISIBLE: u8 = 12;

pub fn run(a: &Path, b: &Path) -> ExitCode {
    let (image_a, image_b) = match (load(a), load(b)) {
        (Ok(x), Ok(y)) => (x, y),
        (Err(e), _) | (_, Err(e)) => {
            eprintln!("image-diff: {e}");
            return ExitCode::FAILURE;
        }
    };
    if (image_a.0, image_a.1) != (image_b.0, image_b.1) {
        eprintln!("image-diff: images have different sizes");
        return ExitCode::FAILURE;
    }
    let stats = compare(&image_a.2, &image_b.2);
    println!(
        "{} vs {}: {:.3} % of pixels visibly changed, mean channel difference {:.3}",
        a.display(),
        b.display(),
        stats.changed_share * 100.0,
        stats.mean_difference
    );
    ExitCode::SUCCESS
}

/// Width, height and RGBA bytes.
fn load(path: &Path) -> Result<(u32, u32, Vec<u8>), String> {
    let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut decoder = png::Decoder::new(BufReader::new(file));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let mut buffer = vec![0; reader.output_buffer_size().unwrap_or(0)];
    let info = reader
        .next_frame(&mut buffer)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if info.color_type != png::ColorType::Rgba {
        return Err(format!("{}: expected an RGBA image", path.display()));
    }
    buffer.truncate(info.buffer_size());
    Ok((info.width, info.height, buffer))
}

#[derive(Debug, PartialEq)]
struct DiffStats {
    changed_share: f64,
    mean_difference: f64,
}

fn compare(a: &[u8], b: &[u8]) -> DiffStats {
    let (mut changed, mut total_difference, mut pixels) = (0u64, 0u64, 0u64);
    for (pa, pb) in a.as_chunks::<4>().0.iter().zip(b.as_chunks::<4>().0) {
        let difference = (0..3).map(|c| pa[c].abs_diff(pb[c])).max().unwrap_or(0);
        changed += u64::from(difference > VISIBLE);
        total_difference += (0..3)
            .map(|c| u64::from(pa[c].abs_diff(pb[c])))
            .sum::<u64>();
        pixels += 1;
    }
    #[allow(clippy::cast_precision_loss)] // pixel counts are far below 2^52
    let (changed, total_difference, pixels) = (
        changed as f64,
        total_difference as f64,
        pixels.max(1) as f64,
    );
    DiffStats {
        changed_share: changed / pixels,
        mean_difference: total_difference / (pixels * 3.0),
    }
}

/// `creeer_tools image-crop <in.png> <out.png> <x> <y> <w> <h> <zoom>`: cuts out a region
/// and enlarges it with nearest-neighbour sampling (every shadow-map texel stays visible).
pub fn crop(input: &Path, output: &Path, region: [u32; 4], zoom: u32) -> ExitCode {
    let (width, height, rgba) = match load(input) {
        Ok(image) => image,
        Err(e) => {
            eprintln!("image-crop: {e}");
            return ExitCode::FAILURE;
        }
    };
    let [x0, y0, w, h] = region;
    if zoom == 0 || x0 + w > width || y0 + h > height {
        eprintln!("image-crop: region outside the {width}x{height} image or zero zoom");
        return ExitCode::FAILURE;
    }
    let (out_w, out_h) = (w * zoom, h * zoom);
    let mut pixels = Vec::with_capacity((out_w * out_h * 4) as usize);
    for y in 0..out_h {
        for x in 0..out_w {
            let i = (((y0 + y / zoom) * width + x0 + x / zoom) * 4) as usize;
            pixels.extend_from_slice(&rgba[i..i + 4]);
        }
    }
    let result = File::create(output)
        .map_err(|e| e.to_string())
        .and_then(|file| {
            let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), out_w, out_h);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
            writer.write_image_data(&pixels).map_err(|e| e.to_string())
        });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("image-crop: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_images_do_not_differ() {
        let image = vec![10, 20, 30, 255, 40, 50, 60, 255];
        let stats = compare(&image, &image);
        assert!(stats.changed_share.abs() < f64::EPSILON);
        assert!(stats.mean_difference.abs() < f64::EPSILON);
    }

    #[test]
    fn only_visible_changes_count() {
        let a = vec![100, 100, 100, 255, 100, 100, 100, 255];
        let b = vec![105, 100, 100, 255, 200, 100, 100, 255];
        let stats = compare(&a, &b);
        assert!((stats.changed_share - 0.5).abs() < 1e-12, "{stats:?}");
    }
}
