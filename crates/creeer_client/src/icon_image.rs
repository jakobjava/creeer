//! The image work of an icon (G1.3h, `docs/art-style.md` §9): its transparency from renders on
//! two backgrounds, the supersampled render reduced to the icon's size, its soft drop shadow,
//! and how much two icons' silhouettes overlap.
//!
//! Transparency by triangulation matting (Smith and Blinn 1996, "Blue screen matting": two
//! known backgrounds determine alpha): a pixel of coverage α over background B shows
//! αF + (1 − α)B, so α = 1 − (on white − on black) / (white − black), and αF is the render
//! on black. Done in linear light, where the blend is linear.

use creeer_art::colour::{decode, encode};
use creeer_render::Image;

/// A linear-light image with premultiplied alpha.
#[derive(Clone, Debug, PartialEq)]
pub struct Premultiplied {
    pub width: u32,
    pub height: u32,
    /// `[r, g, b, a]` per pixel, colour already multiplied by alpha.
    pub pixels: Vec<[f64; 4]>,
}

impl Premultiplied {
    /// The pixels of the same render on black and on a white background that shows `white`
    /// (linear) in the image.
    pub fn matted(on_black: &Image, on_white: &Image, white: f64) -> Self {
        let linear = |c: u8| decode(f64::from(c) / 255.0);
        let pixels = (on_black.rgba.chunks(4).zip(on_white.rgba.chunks(4)))
            .map(|(b, w)| {
                // The mean over the channels: the background is grey, so each says the same.
                let seen: f64 = (0..3).map(|i| linear(w[i]) - linear(b[i])).sum::<f64>() / 3.0;
                let alpha = (1.0 - seen / white).clamp(0.0, 1.0);
                let c = |i: usize| linear(b[i]).min(alpha);
                [c(0), c(1), c(2), alpha]
            })
            .collect();
        Self {
            width: on_black.width,
            height: on_black.height,
            pixels,
        }
    }

    /// Reduced by a whole `factor`, each pixel the mean of the `factor²` it covers (a box
    /// filter: the supersampled render's samples, equally weighted).
    pub fn reduced(&self, factor: u32) -> Self {
        let (width, height) = (self.width / factor, self.height / factor);
        let mut pixels = vec![[0.0; 4]; (width * height) as usize];
        let weight = 1.0 / f64::from(factor * factor);
        for y in 0..height * factor {
            for x in 0..width * factor {
                let from = self.pixels[(y * self.width + x) as usize];
                let to = &mut pixels[((y / factor) * width + x / factor) as usize];
                for i in 0..4 {
                    to[i] += from[i] * weight;
                }
            }
        }
        Self {
            width,
            height,
            pixels,
        }
    }

    /// Over a soft drop shadow: its silhouette moved by `offset` pixels (right, down), blurred
    /// by a Gaussian of `sigma` pixels, black at `opacity`.
    pub fn shadowed(&self, offset: (f64, f64), sigma: f64, opacity: f64) -> Self {
        let (w, h) = (self.width as usize, self.height as usize);
        // The silhouette, moved (to the nearest pixel), then blurred along rows and columns.
        #[allow(clippy::cast_possible_truncation)] // a few pixels
        let (dx, dy) = (offset.0.round() as isize, offset.1.round() as isize);
        let alpha = |x: isize, y: isize| {
            let inside = x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h;
            if inside {
                self.pixels[y as usize * w + x as usize][3]
            } else {
                0.0
            }
        };
        #[allow(clippy::cast_possible_wrap)] // icon sizes
        let moved: Vec<f64> = (0..w * h)
            .map(|i| alpha((i % w) as isize - dx, (i / w) as isize - dy))
            .collect();
        let shadow = blurred(&blurred(&moved, w, h, sigma, (1, 0)), w, h, sigma, (0, 1));
        let pixels = (self.pixels.iter().zip(shadow))
            .map(|(&[r, g, b, a], s)| [r, g, b, a + opacity * s * (1.0 - a)])
            .collect();
        Self {
            width: self.width,
            height: self.height,
            pixels,
        }
    }

    /// As an sRGB image with straight alpha.
    pub fn image(&self) -> Image {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // in 0..=255
        let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        let rgba = (self.pixels.iter())
            .flat_map(|&[r, g, b, a]| {
                let straight = |c: f64| if a > 0.0 { encode(c / a) } else { 0.0 };
                [
                    byte(straight(r)),
                    byte(straight(g)),
                    byte(straight(b)),
                    byte(a),
                ]
            })
            .collect();
        Image {
            width: self.width,
            height: self.height,
            rgba,
        }
    }
}

/// `values` (`w` × `h`) blurred by a Gaussian of `sigma` pixels along `step`, cut at 3σ.
fn blurred(values: &[f64], w: usize, h: usize, sigma: f64, step: (usize, usize)) -> Vec<f64> {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a few pixels
    let reach = (3.0 * sigma).ceil() as isize;
    let kernel: Vec<f64> = (-reach..=reach)
        .map(|k| {
            let x = k as f64 / sigma;
            creeer_math::det::exp(-0.5 * x * x)
        })
        .collect();
    let total: f64 = kernel.iter().sum();
    let mut out = vec![0.0; values.len()];
    for y in 0..h {
        for x in 0..w {
            let mut sum = 0.0;
            for (k, weight) in (-reach..=reach).zip(&kernel) {
                #[allow(clippy::cast_possible_wrap)] // icon sizes
                let (sx, sy) = (
                    x as isize + k * step.0 as isize,
                    y as isize + k * step.1 as isize,
                );
                #[allow(clippy::cast_possible_wrap)]
                if sx >= 0 && sy >= 0 && sx < w as isize && sy < h as isize {
                    sum += weight * values[sy as usize * w + sx as usize];
                }
            }
            out[y * w + x] = sum / total;
        }
    }
    out
}

/// How much the silhouettes (alpha over a half) of two icons of one size overlap: their
/// intersection over their union.
#[cfg(test)]
pub fn overlap(a: &Image, b: &Image) -> f64 {
    let solid = |image: &Image| image.rgba.chunks(4).map(|p| p[3] > 127).collect::<Vec<_>>();
    let (a, b) = (solid(a), solid(b));
    let both = a.iter().zip(&b).filter(|(x, y)| **x && **y).count();
    let either = a.iter().zip(&b).filter(|(x, y)| **x || **y).count();
    #[allow(clippy::cast_precision_loss)] // pixel counts
    let iou = both as f64 / either.max(1) as f64;
    iou
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(pixels: &[[u8; 3]]) -> Image {
        Image {
            width: u32::try_from(pixels.len()).expect("few"),
            height: 1,
            rgba: pixels
                .iter()
                .flat_map(|p| [p[0], p[1], p[2], 255])
                .collect(),
        }
    }

    #[test]
    fn two_backgrounds_say_what_covers_them() {
        // Background, the object, and the object half covering the pixel, on black and white.
        let grey = 0.5;
        let on = |b: f64, f: f64, alpha: f64| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let byte = (encode(alpha * f + (1.0 - alpha) * b) * 255.0).round() as u8;
            [byte; 3]
        };
        let black = image(&[on(0.0, 0.2, 0.0), on(0.0, 0.2, 1.0), on(0.0, 0.2, 0.5)]);
        let white = image(&[on(grey, 0.2, 0.0), on(grey, 0.2, 1.0), on(grey, 0.2, 0.5)]);
        let matted = Premultiplied::matted(&black, &white, grey);
        let alphas: Vec<f64> = matted.pixels.iter().map(|p| p[3]).collect();
        for (got, want) in alphas.iter().zip([0.0, 1.0, 0.5]) {
            assert!((got - want).abs() < 0.01, "{alphas:?}");
        }
        // Its colour, straight again: the object's own.
        let icon = matted.image();
        let object = (encode(0.2) * 255.0).round();
        for x in 1..3 {
            assert!(
                (f64::from(icon.pixel(x, 0)[0]) - object).abs() <= 2.0,
                "{icon:?}"
            );
        }
        assert_eq!(icon.pixel(0, 0)[3], 0);
    }

    #[test]
    fn reducing_averages_and_the_shadow_falls_behind() {
        let mut big = Premultiplied {
            width: 8,
            height: 8,
            pixels: vec![[0.0; 4]; 64],
        };
        // A solid 4 × 4 square in the top left quarter.
        for y in 0..4 {
            for x in 0..4 {
                big.pixels[y * 8 + x] = [0.25, 0.25, 0.25, 1.0];
            }
        }
        let small = big.reduced(4);
        assert_eq!((small.width, small.height), (2, 2));
        assert_eq!(small.pixels[0], [0.25, 0.25, 0.25, 1.0]);
        assert_eq!(small.pixels[3], [0.0; 4]);
        // The shadow: where the square is not, below right of it; never over it.
        let shadowed = big.shadowed((2.0, 2.0), 1.0, 0.5);
        assert_eq!(shadowed.pixels[0], big.pixels[0]);
        let behind = shadowed.pixels[5 * 8 + 5][3];
        assert!(behind > 0.05 && behind < 0.5, "{behind}");
        assert!(shadowed.pixels[7][3] < 0.01, "next to nothing up and right");
    }

    #[test]
    fn overlap_is_intersection_over_union() {
        let a = Image {
            width: 4,
            height: 1,
            rgba: [[0, 0, 0, 255], [0, 0, 0, 255], [0, 0, 0, 255], [0, 0, 0, 0]].concat(),
        };
        let b = Image {
            rgba: [[0, 0, 0, 0], [0, 0, 0, 255], [0, 0, 0, 255], [0, 0, 0, 255]].concat(),
            ..a.clone()
        };
        assert!((overlap(&a, &b) - 0.5).abs() < 1e-12);
        assert!((overlap(&a, &a) - 1.0).abs() < 1e-12);
    }
}
