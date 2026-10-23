//! Golden images (G1.3h, `docs/art-style.md` §11): a render held to an approved one by how
//! different the two look, not by their bytes, since rendering differs a little between GPUs and
//! drivers. Each pixel's colour difference is CIEDE2000 (`creeer_art::colour`): about 2.3 is
//! the least people notice (Sharma 2003; Mahy et al. 1994). An image passes while its mean
//! difference stays under half that and no more than 0.1 % of its pixels differ noticeably.
//! Measured (G1.3h): the style board drawn by WARP against goldens from an RTX GPU differs by
//! a mean of 0.04–0.08 with no pixel noticeably different; the sun 4° lower is caught, and so
//! is the sphere's paint changed by about the least noticeable difference (#6f7a6c to #747a6c:
//! it covers 0.45 % of the image, which 0.5 % let through).

use creeer_art::colour::{Srgb, ciede2000, decode, encode};
use creeer_render::Image;

/// The least colour difference people notice (ΔE₀₀).
pub const NOTICEABLE: f64 = 2.3;
/// The most an image may differ: its mean ΔE₀₀, and the share of its pixels past noticeable.
pub const MEAN: f64 = 0.5 * NOTICEABLE;
pub const SHARE: f64 = 0.001;

/// How two images differ.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Difference {
    pub mean: f64,
    /// The share of pixels whose difference is noticeable.
    pub noticed: f64,
}

impl Difference {
    /// Whether it is within what a golden image allows.
    pub fn passes(&self) -> bool {
        self.mean <= MEAN && self.noticed <= SHARE
    }
}

/// `image` at half its width and height, each pixel the mean of four in linear light: what a
/// golden image keeps (a quarter of the bytes in the repository, and the same at the size the
/// look is judged by).
pub fn halved(image: &Image) -> Image {
    let (width, height) = (image.width / 2, image.height / 2);
    let at = |x: u32, y: u32, i: usize| {
        let byte = image.rgba[((y * image.width + x) * 4) as usize + i];
        if i == 3 {
            f64::from(byte) / 255.0
        } else {
            decode(f64::from(byte) / 255.0)
        }
    };
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            for i in 0..4 {
                let mean = (at(2 * x, 2 * y, i)
                    + at(2 * x + 1, 2 * y, i)
                    + at(2 * x, 2 * y + 1, i)
                    + at(2 * x + 1, 2 * y + 1, i))
                    / 4.0;
                let value = if i == 3 { mean } else { encode(mean) };
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // in 0..=255
                rgba.push((value * 255.0).round() as u8);
            }
        }
    }
    Image {
        width,
        height,
        rgba,
    }
}

/// How `image` differs from `golden` (none if their sizes differ).
pub fn compare(image: &Image, golden: &Image) -> Option<Difference> {
    if (image.width, image.height) != (golden.width, golden.height) {
        return None;
    }
    let (mut sum, mut noticed) = (0.0, 0usize);
    let pixels = image.rgba.chunks(4).zip(golden.rgba.chunks(4));
    for (a, b) in pixels {
        if a[..3] == b[..3] {
            continue;
        }
        let d = ciede2000(
            Srgb([a[0], a[1], a[2]]).lab(),
            Srgb([b[0], b[1], b[2]]).lab(),
        );
        sum += d;
        if d > NOTICEABLE {
            noticed += 1;
        }
    }
    #[allow(clippy::cast_precision_loss)] // pixel counts
    let n = (image.width as usize * image.height as usize).max(1) as f64;
    #[allow(clippy::cast_precision_loss)]
    let noticed = noticed as f64 / n;
    Some(Difference {
        mean: sum / n,
        noticed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filled(rgb: [u8; 3]) -> Image {
        Image {
            width: 10,
            height: 10,
            rgba: [rgb[0], rgb[1], rgb[2], 255].repeat(100),
        }
    }

    #[test]
    fn an_image_differs_by_how_different_it_looks() {
        let grey = filled([120, 120, 120]);
        let same = compare(&grey, &grey).expect("same size");
        assert_eq!(
            same,
            Difference {
                mean: 0.0,
                noticed: 0.0
            }
        );
        // One level lighter everywhere: under what people notice.
        let lighter = compare(&filled([121, 121, 121]), &grey).expect("same size");
        assert!(lighter.passes() && lighter.mean < 0.5, "{lighter:?}");
        // A red tint: noticed everywhere.
        let red = compare(&filled([140, 110, 110]), &grey).expect("same size");
        assert!(!red.passes() && red.noticed > 0.99, "{red:?}");
        // One pixel in a hundred wildly off: past the share allowed.
        let mut spot = grey.clone();
        spot.rgba[..3].copy_from_slice(&[255, 0, 0]);
        let spotted = compare(&spot, &grey).expect("same size");
        assert!((spotted.noticed - 0.01).abs() < 1e-12 && !spotted.passes());
        assert!(
            compare(
                &grey,
                &Image {
                    width: 5,
                    ..grey.clone()
                }
            )
            .is_none()
        );
    }

    #[test]
    fn halving_averages_light() {
        // Black and white pixels side by side: half as much light, not the byte between.
        let image = Image {
            width: 2,
            height: 2,
            rgba: [[0, 0, 0, 255], [255, 255, 255, 255]].concat().repeat(2),
        };
        let half = halved(&image);
        assert_eq!((half.width, half.height), (1, 1));
        assert_eq!(half.pixel(0, 0), [188, 188, 188, 255]);
    }
}
