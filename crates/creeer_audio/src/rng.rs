//! A small, fast random source for the audio thread's noise and grains (xorshift64*: Vigna,
//! "An experimental exploration of Marsaglia's xorshift generators, scrambled", 2016). Seeded,
//! so a rendered sound is the same every time.

/// A random source.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    /// A source seeded with `seed` (any value).
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    /// The next 64 random bits.
    pub fn bits(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform in [0, 1).
    pub fn unit(&mut self) -> f32 {
        // The top 24 bits: every value exactly representable.
        #[allow(clippy::cast_precision_loss)] // 24 bits fit an f32 exactly
        let u = (self.bits() >> 40) as f32;
        u / 16_777_216.0
    }

    /// Uniform in [−1, 1).
    pub fn signed(&mut self) -> f32 {
        2.0 * self.unit() - 1.0
    }

    /// A value of a power law: at least 1, its density falling as x^(−exponent) (exponent > 1),
    /// at most `most`. The energies of cracking bursts follow such a law (Sethna, Dahmen and
    /// Myers 2001; exponent about 1.3 for paper: Salminen, Tolvanen and Alava 2002).
    pub fn power_law(&mut self, exponent: f32, most: f32) -> f32 {
        let u = self.unit().max(1e-7);
        u.powf(-1.0 / (exponent - 1.0)).min(most)
    }
}

#[cfg(test)]
mod tests {
    use super::Rng;

    #[test]
    fn units_are_uniform_and_seeded() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        let n = 100_000;
        let mut sum = 0.0;
        for _ in 0..n {
            let x = a.unit();
            assert!((0.0..1.0).contains(&x));
            assert!((x - b.unit()).abs() < f32::EPSILON);
            sum += f64::from(x);
        }
        assert!((sum / f64::from(n) - 0.5).abs() < 0.01, "{sum}");
    }

    #[test]
    fn a_power_law_is_at_least_one_and_capped() {
        let mut r = Rng::new(3);
        let draws: Vec<f32> = (0..10_000).map(|_| r.power_law(1.5, 1000.0)).collect();
        assert!(draws.iter().all(|&x| (1.0..=1000.0).contains(&x)));
        // Half of a law of exponent 1.5 lies below 2^2 = 4 (its median: u^(−2) = 4 at u = ½).
        let below = draws.iter().filter(|&&x| x < 4.0).count();
        assert!((4500..5500).contains(&below), "{below}");
    }
}
