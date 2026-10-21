//! Colour science against published references.

use crate::colour::{Lab, Srgb, ciede2000, decode, encode};
use crate::vision::Vision;

/// Sharma, Wu and Dalal (2005), "The CIEDE2000 Color-Difference Formula: Implementation Notes,
/// Supplementary Test Data, and Mathematical Observations", Table 1: 34 pairs (L₁ a₁ b₁ L₂ a₂
/// b₂ ΔE₀₀), from the authors' `ciede2000testdata.txt`.
const SHARMA: &str = include_str!("sharma2005.txt");

#[test]
fn ciede2000_matches_sharma_wu_dalal() {
    let mut pairs = 0;
    for line in SHARMA.lines().filter(|l| !l.trim().is_empty()) {
        let v: Vec<f64> = line
            .split_whitespace()
            .map(|x| x.parse().expect("a number"))
            .collect();
        let x = Lab {
            l: v[0],
            a: v[1],
            b: v[2],
        };
        let y = Lab {
            l: v[3],
            a: v[4],
            b: v[5],
        };
        // The table gives four decimals; it is symmetric in its two colours.
        for d in [ciede2000(x, y), ciede2000(y, x)] {
            assert!(
                (d - v[6]).abs() < 1e-4,
                "pair {}: {d:.6} against {}",
                pairs + 1,
                v[6]
            );
        }
        pairs += 1;
    }
    assert_eq!(pairs, 34);
}

#[test]
fn lab_of_known_colours() {
    // White is L* 100 and neutral; black is 0.
    let white = Srgb([255, 255, 255]).lab();
    assert!((white.l - 100.0).abs() < 1e-3, "{white:?}");
    assert!(white.chroma() < 1e-2, "{white:?}");
    assert!(Srgb([0, 0, 0]).lab().l.abs() < 1e-12);
    // sRGB red (Lindbloom's calculator, D65): L* 53.2408, a* 80.0925, b* 67.2032.
    let red = Srgb([255, 0, 0]).lab();
    assert!((red.l - 53.2408).abs() < 1e-3, "{red:?}");
    assert!((red.a - 80.0925).abs() < 1e-3, "{red:?}");
    assert!((red.b - 67.2032).abs() < 1e-3, "{red:?}");
    // Middle grey 50 % in linear light: L* 76.0693 (Y = 0.5: 116·0.5^(1/3) − 16).
    let grey = Lab::from_linear([0.5, 0.5, 0.5]);
    assert!((grey.l - 76.0693).abs() < 1e-3, "{grey:?}");
}

#[test]
fn srgb_round_trips() {
    for c in 0..=255_u8 {
        let s = Srgb([c, 255 - c, c / 2]);
        assert_eq!(Srgb::from_linear(s.linear()), s);
        assert_eq!(Srgb::parse(&s.to_string()), Some(s));
    }
    for x in [0.0, 0.002, 0.003_130_8, 0.04, 0.5, 1.0] {
        assert!((decode(encode(x)) - x).abs() < 1e-12, "{x}");
    }
    for bad in ["8a8d90", "#8a8d9", "#8a8d9g", "#8a8d900", ""] {
        assert_eq!(Srgb::parse(bad), None, "{bad:?}");
    }
}

#[test]
fn colour_blind_simulation_keeps_greys_and_confuses_the_known_pairs() {
    // Machado et al.'s rows each sum to 1: greys look the same to everyone.
    for vision in Vision::ALL {
        for row in vision.matrix() {
            assert!((row.iter().sum::<f64>() - 1.0).abs() < 2e-6, "{vision:?}");
        }
        for c in [0_u8, 64, 128, 200, 255] {
            assert_eq!(vision.sees(Srgb([c, c, c])), Srgb([c, c, c]), "{vision:?}");
        }
    }
    // Red against green and blue against violet: distinct to normal vision, confused by
    // red-green dichromats (the pairs the colour-blind palettes avoid).
    let d =
        |v: Vision, x: [u8; 3], y: [u8; 3]| ciede2000(v.sees(Srgb(x)).lab(), v.sees(Srgb(y)).lab());
    let (red, green) = ([200, 60, 50], [90, 140, 50]);
    let (blue, violet) = ([61, 127, 217], [142, 91, 214]);
    assert!(d(Vision::Normal, red, green) > 40.0);
    assert!(d(Vision::Normal, blue, violet) > 15.0);
    // Deuteranopes all but merge the two (ΔE₀₀ 1.3); protanopes see them about a third as far
    // apart as normal vision does (6.8).
    assert!(d(Vision::Deuteranopia, blue, violet) < 3.0);
    for v in [Vision::Protanopia, Vision::Deuteranopia] {
        let ratio = d(v, blue, violet) / d(Vision::Normal, blue, violet);
        assert!(ratio < 0.4, "{v:?}: {ratio}");
        assert!(
            d(v, red, green) < d(Vision::Normal, red, green) / 3.0,
            "{v:?}: {}",
            d(v, red, green)
        );
    }
}
