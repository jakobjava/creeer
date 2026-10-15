//! Engineering overlays: colouring parts by a measured value (stress, temperature, voltage…).
//!
//! The palettes are chosen for reading data correctly, not for looks:
//! - [`Palette::Sequential`] is *viridis*: brightness rises monotonically and evenly, so equal
//!   steps in value look like equal steps in colour, also for colour-blind viewers.
//! - [`Palette::Diverging`] is *cool-warm* (Moreland): blue below the midpoint, red above,
//!   neutral at zero, for signed values such as tension (+) and compression (−).
//!
//! `mesh.wgsl` evaluates the same formulas on the GPU; the CPU versions draw the legend.

/// Colour scale of an overlay.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Palette {
    /// Magnitudes (temperature, stress magnitude, pressure): viridis.
    #[default]
    Sequential,
    /// Signed values around zero (axial stress, voltage difference): cool-warm.
    Diverging,
}

impl Palette {
    /// Index used by the shader.
    pub(crate) fn shader_id(self) -> f32 {
        match self {
            Self::Sequential => 0.0,
            Self::Diverging => 1.0,
        }
    }

    /// sRGB colour (0–1) for `t` in [0, 1].
    pub fn color(self, t: f32) -> [f32; 3] {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Sequential => viridis(t),
            Self::Diverging => cool_warm(t),
        }
    }
}

/// Which value range to show and how.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Heatmap {
    /// Value shown at the bottom of the scale.
    pub min: f32,
    /// Value shown at the top of the scale.
    pub max: f32,
    /// Colour scale.
    pub palette: Palette,
}

impl Heatmap {
    /// Position of `value` on the scale, clamped to [0, 1].
    pub fn normalize(&self, value: f32) -> f32 {
        let span = self.max - self.min;
        if span.abs() < f32::EPSILON {
            return 0.5;
        }
        ((value - self.min) / span).clamp(0.0, 1.0)
    }

    /// sRGB colour of `value`.
    pub fn color(&self, value: f32) -> [f32; 3] {
        self.palette.color(self.normalize(value))
    }
}

/// Polynomial fit of matplotlib's viridis (sRGB), max error about 0.01.
fn viridis(t: f32) -> [f32; 3] {
    const C: [[f32; 3]; 7] = [
        [0.277_727_33, 0.005_407_344_5, 0.334_099_8],
        [0.105_093_04, 1.404_613_5, 1.384_590_2],
        [-0.330_861_83, 0.214_847_56, 0.095_095_16],
        [-4.634_230_6, -5.799_101, -19.332_441],
        [6.228_27, 14.179_933, 56.690_55],
        [4.776_385, -13.745_145, -65.353_03],
        [-5.435_456, 4.645_852_6, 26.312_435],
    ];
    std::array::from_fn(|channel| {
        C.iter()
            .rev()
            .fold(0.0, |acc, coefficients| acc * t + coefficients[channel])
            .clamp(0.0, 1.0)
    })
}

/// Moreland's cool-warm endpoints with a neutral grey centre (sRGB).
fn cool_warm(t: f32) -> [f32; 3] {
    const COOL: [f32; 3] = [0.230, 0.299, 0.754];
    const NEUTRAL: [f32; 3] = [0.865, 0.865, 0.865];
    const WARM: [f32; 3] = [0.706, 0.016, 0.150];
    let (from, to, s) = if t < 0.5 {
        (COOL, NEUTRAL, t * 2.0)
    } else {
        (NEUTRAL, WARM, (t - 0.5) * 2.0)
    };
    std::array::from_fn(|c| from[c] + (to[c] - from[c]) * s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(c: [f32; 3]) -> f32 {
        0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
    }

    #[test]
    fn viridis_matches_its_reference_endpoints() {
        let (start, end) = (viridis(0.0), viridis(1.0));
        for (got, want) in start.iter().zip([0.267, 0.005, 0.329]) {
            assert!((got - want).abs() < 0.015, "start {start:?}");
        }
        for (got, want) in end.iter().zip([0.993, 0.906, 0.144]) {
            assert!((got - want).abs() < 0.015, "end {end:?}");
        }
    }

    /// The property that makes viridis honest: brightness never goes back down.
    #[test]
    fn viridis_brightness_rises_monotonically() {
        let mut previous = -1.0;
        for i in 0..=100u8 {
            let l = luminance(viridis(f32::from(i) / 100.0));
            assert!(l > previous, "luminance dips at {i}%");
            previous = l;
        }
    }

    #[test]
    fn diverging_scale_is_neutral_in_the_middle_and_blue_red_at_the_ends() {
        let mid = Palette::Diverging.color(0.5);
        assert!(
            (mid[0] - mid[2]).abs() < 1e-6,
            "middle not neutral: {mid:?}"
        );
        let (low, high) = (Palette::Diverging.color(0.0), Palette::Diverging.color(1.0));
        assert!(low[2] > low[0], "low end not blue");
        assert!(high[0] > high[2], "high end not red");
    }

    #[test]
    fn values_map_onto_the_scale_and_clamp() {
        let heat = Heatmap {
            min: 20.0,
            max: 420.0,
            palette: Palette::Sequential,
        };
        assert!((heat.normalize(220.0) - 0.5).abs() < 1e-6);
        assert!(heat.normalize(-100.0).abs() < f32::EPSILON);
        assert!((heat.normalize(9_999.0) - 1.0).abs() < f32::EPSILON);
    }
}
