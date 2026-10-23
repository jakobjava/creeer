//! A small deterministic generator for the scatter: the same numbers on every machine.

use creeer_math::det;

/// A small deterministic generator (SplitMix64): the same numbers on every machine.
pub(crate) struct Rng(u64);

impl Rng {
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// In [0, 1).
    pub(crate) fn unit(&mut self) -> f64 {
        #[allow(clippy::cast_precision_loss)] // 53 bits
        let x = (self.next() >> 11) as f64;
        x / (1u64 << 53) as f64
    }

    /// A Poisson draw of mean `mean` (Knuth's, by products: means here are small).
    pub(crate) fn poisson(&mut self, mean: f64) -> usize {
        let limit = det::exp(-mean);
        let (mut k, mut p) = (0, 1.0);
        loop {
            p *= self.unit();
            if p <= limit || k > 4096 {
                return k;
            }
            k += 1;
        }
    }
}
