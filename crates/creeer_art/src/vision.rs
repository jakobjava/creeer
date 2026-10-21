//! Colour-vision deficiency simulated as Machado, Oliveira and Fernandes (2009) model it
//! ("A Physiologically-based Model for Simulation of Color Vision Deficiency", IEEE TVCG 15,
//! doi 10.1109/TVCG.2009.113): a 3×3 matrix on linear sRGB per deficiency and severity. The
//! matrices are the paper's for severity 1 (dichromacy), as transcribed in colorspacious
//! (`cvd.py`, MIT); `art-style.md` §3.6 asks player colours to stay apart under both.

use crate::colour::Srgb;

/// Who sees the colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Vision {
    /// Normal colour vision.
    Normal,
    /// Protanopia: no long-wavelength (L) cones.
    Protanopia,
    /// Deuteranopia: no medium-wavelength (M) cones, the most common dichromacy.
    Deuteranopia,
}

impl Vision {
    /// Every vision the checks consider.
    pub const ALL: [Self; 3] = [Self::Normal, Self::Protanopia, Self::Deuteranopia];

    /// Machado et al.'s matrix on linear sRGB, severity 1.
    pub fn matrix(self) -> [[f64; 3]; 3] {
        match self {
            Self::Normal => [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            Self::Protanopia => [
                [0.152_286, 1.052_583, -0.204_868],
                [0.114_503, 0.786_281, 0.099_216],
                [-0.003_882, -0.048_116, 1.051_998],
            ],
            Self::Deuteranopia => [
                [0.367_322, 0.860_646, -0.227_968],
                [0.280_085, 0.672_501, 0.047_413],
                [-0.011_820, 0.042_940, 0.968_881],
            ],
        }
    }

    /// The linear sRGB channels `colour` appears as (unclamped).
    pub fn sees_linear(self, colour: Srgb) -> [f64; 3] {
        let rgb = colour.linear();
        self.matrix()
            .map(|row| row[0] * rgb[0] + row[1] * rgb[1] + row[2] * rgb[2])
    }

    /// The sRGB colour `colour` appears as, clamped to the gamut and rounded to 8 bits.
    pub fn sees(self, colour: Srgb) -> Srgb {
        Srgb::from_linear(self.sees_linear(colour))
    }
}
