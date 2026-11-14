//! The last stage of the mix: a peak limiter that looks 64 samples ahead (the gain falls before a
//! peak arrives, not on it, so it does not crackle), and a soft clip after it, so the output never
//! reaches full scale whatever is played.

/// Samples the limiter looks ahead (1.3 ms at 48 kHz).
const AHEAD: usize = 64;
/// Where the soft clip begins (of full scale).
const KNEE: f32 = 0.9;

/// The limiter.
#[derive(Clone, Debug)]
pub struct Limiter {
    delay: [[f32; 2]; AHEAD],
    at: usize,
    /// The gain wanted (falls at once, recovers slowly) and the gain applied (follows within the
    /// look-ahead).
    want: f32,
    gain: f32,
    recover: f32,
    attack: f32,
    /// The lowest gain it has applied since asked (1: never limited).
    pub least: f32,
}

impl Limiter {
    /// A limiter for `rate` samples a second: recovering over a fifth of a second.
    pub fn new(rate: f32) -> Self {
        Self {
            delay: [[0.0; 2]; AHEAD],
            at: 0,
            want: 1.0,
            gain: 1.0,
            recover: 1.0 - (-1.0 / (0.2 * rate)).exp(),
            #[allow(clippy::cast_precision_loss)] // 64
            attack: 1.0 - (-6.0 / AHEAD as f32).exp(),
            least: 1.0,
        }
    }

    /// One stereo sample in, the one `AHEAD` samples before it out, limited.
    #[inline]
    pub fn run(&mut self, x: [f32; 2]) -> [f32; 2] {
        let peak = x[0].abs().max(x[1].abs());
        let target = if peak > KNEE { KNEE / peak } else { 1.0 };
        self.want = (self.want + (1.0 - self.want) * self.recover).min(target);
        self.gain += (self.want - self.gain)
            * if self.want < self.gain {
                self.attack
            } else {
                1.0
            };
        self.least = self.least.min(self.gain);
        let out = self.delay[self.at];
        self.delay[self.at] = x;
        self.at = (self.at + 1) % AHEAD;
        out.map(|s| soft_clip(s * self.gain))
    }
}

/// Unchanged below the knee, bent smoothly towards full scale above it, never reaching it.
#[inline]
pub fn soft_clip(x: f32) -> f32 {
    let a = x.abs();
    if a <= KNEE {
        x
    } else {
        let bent = KNEE + (1.0 - KNEE) * ((a - KNEE) / (1.0 - KNEE)).tanh();
        bent.min(0.999_99).copysign(x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_comes_out_at_full_scale_whatever_goes_in() {
        let mut l = Limiter::new(48_000.0);
        let mut most = 0.0f32;
        for k in 0..48_000 {
            #[allow(clippy::cast_precision_loss)]
            let t = k as f32 / 48_000.0;
            let x = 40.0 * (t * 700.0).sin();
            let y = l.run([x, -x]);
            most = most.max(y[0].abs()).max(y[1].abs());
        }
        assert!(most < 1.0, "{most}");
        assert!(l.least < 0.05);
        // Quiet sound passes untouched (once through the look-ahead).
        let mut l = Limiter::new(48_000.0);
        let mut out = Vec::new();
        for k in 0..200 {
            #[allow(clippy::cast_precision_loss)]
            out.push(l.run([0.1 * (k as f32 * 0.1).sin(), 0.0])[0]);
        }
        #[allow(clippy::cast_precision_loss)]
        let first = 0.1 * ((150 - AHEAD) as f32 * 0.1).sin();
        assert!((out[150] - first).abs() < 1e-6);
    }
}
