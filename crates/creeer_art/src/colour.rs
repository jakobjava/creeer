//! Colour science for the style's checks (`docs/art-style.md` §4): sRGB as authored, CIELAB
//! under D65, and the CIEDE2000 colour difference as Sharma, Wu and Dalal (2005) give it.
//! Everything is f64 through `creeer_math::det`, so the checks agree on every OS.

use creeer_math::det;
use std::fmt;

/// An sRGB colour as authored, 8 bits a channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Srgb(pub [u8; 3]);

impl Srgb {
    /// Parses `#rrggbb`.
    pub fn parse(text: &str) -> Option<Self> {
        let hex = text.strip_prefix('#')?;
        if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let [_, r, g, b] = u32::from_str_radix(hex, 16).ok()?.to_be_bytes();
        Some(Self([r, g, b]))
    }

    /// Its linear-light channels (0–1).
    pub fn linear(self) -> [f64; 3] {
        self.0.map(|c| decode(f64::from(c) / 255.0))
    }

    /// The nearest 8-bit colour to linear channels `rgb` (clamped to 0–1).
    pub fn from_linear(rgb: [f64; 3]) -> Self {
        Self(rgb.map(|c| {
            let code = (encode(c.clamp(0.0, 1.0)) * 255.0).round();
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0–255
            let byte = code as u8;
            byte
        }))
    }

    /// Its CIELAB coordinates (D65).
    pub fn lab(self) -> Lab {
        Lab::from_linear(self.linear())
    }
}

impl fmt::Display for Srgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [r, g, b] = self.0;
        write!(f, "#{r:02x}{g:02x}{b:02x}")
    }
}

/// The sRGB transfer function, encoded (0–1) to linear.
pub fn decode(c: f64) -> f64 {
    if c <= 0.040_45 {
        c / 12.92
    } else {
        det::pow((c + 0.055) / 1.055, 2.4)
    }
}

/// The sRGB transfer function, linear to encoded (0–1).
pub fn encode(c: f64) -> f64 {
    if c <= 0.003_130_8 {
        12.92 * c
    } else {
        1.055 * det::pow(c, 1.0 / 2.4) - 0.055
    }
}

/// A colour in CIELAB (D65 white).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lab {
    /// Lightness L\* (0 black, 100 white).
    pub l: f64,
    pub a: f64,
    pub b: f64,
}

/// D65's white point (CIE 1931 2°, Y = 1), as the sRGB standard (IEC 61966-2-1) has it.
const WHITE: [f64; 3] = [0.950_47, 1.0, 1.088_83];

impl Lab {
    /// From linear sRGB channels.
    pub fn from_linear([r, g, b]: [f64; 3]) -> Self {
        // IEC 61966-2-1's matrix, linear sRGB to XYZ.
        let x = 0.412_456_4 * r + 0.357_576_1 * g + 0.180_437_5 * b;
        let y = 0.212_672_9 * r + 0.715_152_2 * g + 0.072_175_0 * b;
        let z = 0.019_333_9 * r + 0.119_192_0 * g + 0.950_304_1 * b;
        // CIE's f with its exact constants: ε = 216/24389, κ = 24389/27.
        let f = |t: f64| {
            if t > 216.0 / 24_389.0 {
                det::cbrt(t)
            } else {
                (24_389.0 / 27.0 * t + 16.0) / 116.0
            }
        };
        let [fx, fy, fz] = [f(x / WHITE[0]), f(y / WHITE[1]), f(z / WHITE[2])];
        Self {
            l: 116.0 * fy - 16.0,
            a: 500.0 * (fx - fy),
            b: 200.0 * (fy - fz),
        }
    }

    /// Chroma C\*ab.
    pub fn chroma(self) -> f64 {
        det::hypot(self.a, self.b)
    }
}

/// x⁷, by multiplication.
fn pow7(x: f64) -> f64 {
    let x2 = x * x;
    x2 * x2 * x2 * x
}

/// Hue angle in degrees, 0–360 (0 for a neutral colour).
fn hue(b: f64, a: f64) -> f64 {
    if a == 0.0 && b == 0.0 {
        return 0.0;
    }
    let h = det::atan2(b, a).to_degrees();
    if h < 0.0 { h + 360.0 } else { h }
}

/// The CIEDE2000 colour difference ΔE₀₀ with kL = kC = kH = 1 (Sharma, Wu and Dalal 2005,
/// equations 1–22).
pub fn ciede2000(x: Lab, y: Lab) -> f64 {
    let c_bar = (x.chroma() + y.chroma()) / 2.0;
    let g = 0.5 * (1.0 - (pow7(c_bar) / (pow7(c_bar) + pow7(25.0))).sqrt());
    let (a1, a2) = ((1.0 + g) * x.a, (1.0 + g) * y.a);
    let (c1, c2) = (det::hypot(a1, x.b), det::hypot(a2, y.b));
    let (h1, h2) = (hue(x.b, a1), hue(y.b, a2));
    let both = c1 * c2 != 0.0;

    let dl = y.l - x.l;
    let dc = c2 - c1;
    let mut dh = 0.0;
    if both {
        dh = h2 - h1;
        if dh > 180.0 {
            dh -= 360.0;
        } else if dh < -180.0 {
            dh += 360.0;
        }
    }
    let d_big_h = 2.0 * (c1 * c2).sqrt() * det::sin((dh / 2.0).to_radians());

    let l_bar = (x.l + y.l) / 2.0;
    let c_bar1 = (c1 + c2) / 2.0;
    let mut h_bar = h1 + h2;
    if both {
        if (h1 - h2).abs() > 180.0 {
            h_bar += if h_bar < 360.0 { 360.0 } else { -360.0 };
        }
        h_bar /= 2.0;
    }
    let cos = |deg: f64| det::cos(deg.to_radians());
    let t =
        1.0 - 0.17 * cos(h_bar - 30.0) + 0.24 * cos(2.0 * h_bar) + 0.32 * cos(3.0 * h_bar + 6.0)
            - 0.20 * cos(4.0 * h_bar - 63.0);
    let d_theta = 30.0 * det::exp(-((h_bar - 275.0) / 25.0) * ((h_bar - 275.0) / 25.0));
    let r_c = 2.0 * (pow7(c_bar1) / (pow7(c_bar1) + pow7(25.0))).sqrt();
    let l50 = (l_bar - 50.0) * (l_bar - 50.0);
    let s_l = 1.0 + 0.015 * l50 / (20.0 + l50).sqrt();
    let s_c = 1.0 + 0.045 * c_bar1;
    let s_h = 1.0 + 0.015 * c_bar1 * t;
    let r_t = -det::sin((2.0 * d_theta).to_radians()) * r_c;
    let (ql, qc, qh) = (dl / s_l, dc / s_c, d_big_h / s_h);
    (ql * ql + qc * qc + qh * qh + r_t * qc * qh).sqrt()
}

/// ΔE₀₀ between two sRGB colours.
pub fn difference(x: Srgb, y: Srgb) -> f64 {
    ciede2000(x.lab(), y.lab())
}
