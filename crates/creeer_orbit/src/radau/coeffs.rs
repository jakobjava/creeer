//! Constants of the 15th-order Gauss–Radau scheme (Everhart 1985).

use std::sync::OnceLock;

/// Substep spacings h₀..h₇ on [0, 1]: the left end and the seven Gauss–Radau nodes (the
/// roots of (P₇ + P₈)/(1 + x) on [−1, 1], mapped to [0, 1]). From Rein & Spiegel (2015),
/// checked against the Legendre polynomials in the tests.
#[allow(clippy::excessive_precision)] // the published digits
pub const H: [f64; 8] = [
    0.0,
    0.056_262_560_536_922_146_465_652_191_031_8,
    0.180_240_691_736_892_364_987_579_942_780,
    0.352_624_717_113_169_637_373_907_769_648,
    0.547_153_626_330_555_383_001_448_554_766,
    0.734_210_177_215_410_531_523_210_605_558,
    0.885_320_946_839_095_768_090_359_771_030,
    0.977_520_613_561_287_501_891_174_488_626,
];

/// Derived constants.
#[derive(Debug)]
pub struct Coefficients {
    /// r[n][j] = 1 / (hₙ − hⱼ) for j < n: the divided differences.
    pub r: [[f64; 8]; 8],
    /// c[k][p]: coefficient of h^p in Nₖ(h) = h·(h − h₁)···(h − h_{k−1}), k = 1..7 (the
    /// Newton basis of the acceleration in powers of h).
    pub c: [[f64; 8]; 8],
}

/// The constants, computed once.
pub fn coefficients() -> &'static Coefficients {
    static COEFFICIENTS: OnceLock<Coefficients> = OnceLock::new();
    COEFFICIENTS.get_or_init(|| {
        let mut r = [[0.0; 8]; 8];
        for n in 1..8 {
            for j in 0..n {
                r[n][j] = 1.0 / (H[n] - H[j]);
            }
        }
        let mut c = [[0.0; 8]; 8];
        // N₁ = h; Nₖ = Nₖ₋₁·(h − hₖ₋₁).
        c[1][1] = 1.0;
        for k in 2..8 {
            for p in 1..=k {
                c[k][p] = c[k - 1][p - 1] - H[k - 1] * c[k - 1][p];
            }
        }
        Coefficients { r, c }
    })
}

/// Binomial coefficient C(n, k) for the small n used here (exact in f64).
pub fn binomial(n: usize, k: usize) -> f64 {
    let mut value = 1.0;
    for i in 0..k {
        #[allow(clippy::cast_precision_loss)] // i, n ≤ 8
        {
            value = value * (n - i) as f64 / (i + 1) as f64;
        }
    }
    value
}
