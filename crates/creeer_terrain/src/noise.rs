//! Deterministic 3D gradient noise (Ken Perlin's "improved noise", 2002) and fractal sums.
//!
//! World generation must give identical terrain for the same seed on every machine, so this
//! uses only exactly-rounded IEEE operations (+, −, ×, floor) and a seeded permutation, no
//! platform math library calls.

/// Seeded 3D Perlin noise.
#[derive(Clone, Debug)]
pub struct Perlin {
    /// Permutation of 0..=255, repeated twice to avoid index wrapping.
    perm: [u8; 512],
}

impl Perlin {
    /// Creates the noise function for `seed`.
    pub fn new(seed: u64) -> Self {
        let mut table: [u8; 256] = std::array::from_fn(|i| u8::try_from(i).expect("i < 256"));
        // Fisher–Yates shuffle driven by SplitMix64.
        let mut state = seed;
        for i in (1..256usize).rev() {
            state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^= z >> 31;
            let j = usize::try_from(z % (i as u64 + 1)).expect("fits");
            table.swap(i, j);
        }
        let mut perm = [0; 512];
        for (i, p) in perm.iter_mut().enumerate() {
            *p = table[i & 255];
        }
        Self { perm }
    }

    /// Noise value at `(x, y, z)`, roughly in [−1, 1], zero at every integer lattice point.
    pub fn sample(&self, x: f64, y: f64, z: f64) -> f64 {
        let (xf, yf, zf) = (x.floor(), y.floor(), z.floor());
        let (xi, yi, zi) = (lattice(xf), lattice(yf), lattice(zf));
        let (x, y, z) = (x - xf, y - yf, z - zf);
        let (u, v, w) = (fade(x), fade(y), fade(z));

        let p = &self.perm;
        let a = usize::from(p[xi]) + yi;
        let (aa, ab) = (usize::from(p[a]) + zi, usize::from(p[a + 1]) + zi);
        let b = usize::from(p[xi + 1]) + yi;
        let (ba, bb) = (usize::from(p[b]) + zi, usize::from(p[b + 1]) + zi);

        lerp(
            w,
            lerp(
                v,
                lerp(u, grad(p[aa], x, y, z), grad(p[ba], x - 1.0, y, z)),
                lerp(
                    u,
                    grad(p[ab], x, y - 1.0, z),
                    grad(p[bb], x - 1.0, y - 1.0, z),
                ),
            ),
            lerp(
                v,
                lerp(
                    u,
                    grad(p[aa + 1], x, y, z - 1.0),
                    grad(p[ba + 1], x - 1.0, y, z - 1.0),
                ),
                lerp(
                    u,
                    grad(p[ab + 1], x, y - 1.0, z - 1.0),
                    grad(p[bb + 1], x - 1.0, y - 1.0, z - 1.0),
                ),
            ),
        )
    }

    /// Fractal Brownian motion: `octaves` layers, each at double the frequency and half the
    /// amplitude of the previous one. Result roughly in [−1, 1].
    pub fn fbm(&self, x: f64, y: f64, z: f64, octaves: u32) -> f64 {
        let (mut sum, mut amplitude, mut frequency, mut norm) = (0.0, 1.0, 1.0, 0.0);
        for _ in 0..octaves {
            sum += amplitude * self.sample(x * frequency, y * frequency, z * frequency);
            norm += amplitude;
            amplitude *= 0.5;
            frequency *= 2.0;
        }
        sum / norm
    }
}

/// Lattice cell index wrapped to 0..=255 (the noise repeats every 256 units).
fn lattice(floor: f64) -> usize {
    #[allow(clippy::cast_possible_truncation)] // wrapped to 8 bits right after
    let i = floor as i64;
    usize::try_from(i.rem_euclid(256)).expect("0..256")
}

/// Quintic smoothstep 6t⁵ − 15t⁴ + 10t³ (continuous second derivative).
fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn lerp(t: f64, a: f64, b: f64) -> f64 {
    a + t * (b - a)
}

/// Dot product with one of 12 edge-direction gradients selected by the hash.
fn grad(hash: u8, x: f64, y: f64, z: f64) -> f64 {
    let h = hash & 15;
    let u = if h < 8 { x } else { y };
    let v = match h {
        0..=3 => y,
        12 | 14 => x,
        _ => z,
    };
    (if h & 1 == 0 { u } else { -u }) + (if h & 2 == 0 { v } else { -v })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_at_lattice_points() {
        let noise = Perlin::new(7);
        for (x, y, z) in [(0.0, 0.0, 0.0), (3.0, -5.0, 12.0), (255.0, 256.0, -1.0)] {
            assert!(noise.sample(x, y, z).abs() < 1e-12);
        }
    }

    #[test]
    fn bounded_and_not_constant() {
        let noise = Perlin::new(42);
        let (mut min, mut max) = (f64::MAX, f64::MIN);
        for i in 0..20_000 {
            let t = f64::from(i) * 0.137;
            let v = noise.fbm(t, t * 0.71 + 3.3, -t * 0.43 + 1.7, 5);
            min = min.min(v);
            max = max.max(v);
        }
        assert!(min >= -1.0 && max <= 1.0, "range [{min}, {max}]");
        assert!(
            max - min > 0.5,
            "noise is suspiciously flat: [{min}, {max}]"
        );
    }

    #[test]
    fn continuous() {
        let noise = Perlin::new(1);
        let (a, b) = (
            noise.sample(1.5, 2.25, 3.125),
            noise.sample(1.500_001, 2.25, 3.125),
        );
        assert!((a - b).abs() < 1e-4);
    }

    #[test]
    fn different_seeds_differ_and_same_seed_repeats() {
        let (a, b) = (Perlin::new(1), Perlin::new(2));
        let p = (10.3, 20.7, 30.1);
        assert!((a.sample(p.0, p.1, p.2) - b.sample(p.0, p.1, p.2)).abs() > 1e-6);
        let a2 = Perlin::new(1);
        assert_eq!(
            a.sample(p.0, p.1, p.2).to_bits(),
            a2.sample(p.0, p.1, p.2).to_bits()
        );
    }
}
