//! A small deterministic random source (SplitMix64): the same seed gives the same system on
//! every platform.

use creeer_math::det;

/// SplitMix64 (Steele, Lea & Flood 2014).
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    /// A generator for `seed`, decorrelated by `stream` (one stream per purpose).
    pub fn new(seed: u64, stream: u64) -> Self {
        let mut rng = Self(seed ^ stream.wrapping_mul(0x9e37_79b9_7f4a_7c15));
        rng.next_u64();
        rng
    }

    /// The next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1).
    pub fn unit(&mut self) -> f64 {
        // The top 53 bits make an exact double in [0, 1).
        #[allow(clippy::cast_precision_loss)] // < 2⁵³: exact
        let bits = (self.next_u64() >> 11) as f64;
        bits / 9_007_199_254_740_992.0
    }

    /// Uniform in [lo, hi).
    pub fn range(&mut self, (lo, hi): (f64, f64)) -> f64 {
        lo + (hi - lo) * self.unit()
    }

    /// Log-uniform in [lo, hi) (both positive): every factor of ten equally likely.
    pub fn log_range(&mut self, (lo, hi): (f64, f64)) -> f64 {
        lo * det::exp(det::ln(hi / lo) * self.unit())
    }

    /// Uniform integer in [lo, hi] (inclusive).
    pub fn count(&mut self, (lo, hi): (u32, u32)) -> u32 {
        let span = u64::from(hi.saturating_sub(lo)) + 1;
        lo + u32::try_from(self.next_u64() % span).expect("span fits u32")
    }

    /// An angle in [0, 2π).
    pub fn angle(&mut self) -> f64 {
        std::f64::consts::TAU * self.unit()
    }

    /// True with probability `p`.
    pub fn chance(&mut self, p: f64) -> bool {
        self.unit() < p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streams_are_reproducible_and_uniform() {
        let mut a = Rng::new(42, 1);
        let mut b = Rng::new(42, 1);
        let mut c = Rng::new(42, 2);
        let mut sum = 0.0;
        let mut differs = false;
        for _ in 0..10_000 {
            let x = a.unit();
            assert_eq!(x.to_bits(), b.unit().to_bits());
            differs |= x.to_bits() != c.unit().to_bits();
            assert!((0.0..1.0).contains(&x));
            sum += x;
        }
        assert!(differs);
        assert!((sum / 10_000.0 - 0.5).abs() < 0.01);
        let mut d = Rng::new(7, 0);
        for _ in 0..1000 {
            assert!((3..=5).contains(&d.count((3, 5))));
        }
    }
}
