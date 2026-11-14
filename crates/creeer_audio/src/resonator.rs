//! A bank of resonators, one per mode (audio thread). Each is a complex one-pole, z ← p·z + g·x
//! with p = e^(−d/fs)·e^(i·2πf/fs), its output the imaginary part: an impulse of 1 rings it as
//! g·e^(−d·t)·sin(2πf·t). The complex form keeps low, long-ringing modes accurate in f32, where
//! a real two-pole's coefficient 2r·cos θ loses them (it rounds to 2).

use crate::modes::{MAX_MODES, Modes};

/// The resonators of one sound.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bank {
    re: [f32; MAX_MODES],
    im: [f32; MAX_MODES],
    pole: [(f32, f32); MAX_MODES],
    gain: [f32; MAX_MODES],
    len: usize,
}

impl Bank {
    /// The resonators of `modes` at `rate` samples a second (modes above 0.45 of it left out).
    pub fn new(modes: &Modes, rate: f32) -> Self {
        let mut bank = Self::default();
        for m in modes.as_slice() {
            if m.freq >= 0.45 * rate || m.amp <= 0.0 {
                continue;
            }
            let r = (-m.decay / rate).exp();
            let theta = std::f32::consts::TAU * m.freq / rate;
            let k = bank.len;
            bank.pole[k] = (r * theta.cos(), r * theta.sin());
            bank.gain[k] = m.amp;
            bank.len += 1;
        }
        bank
    }

    /// One sample: the input `x` (an impulse of 1 rings each mode at its amplitude), and the
    /// sum of the modes.
    #[inline]
    pub fn tick(&mut self, x: f32) -> f32 {
        let mut out = 0.0;
        for k in 0..self.len {
            let (pr, pi) = self.pole[k];
            let (re, im) = (self.re[k], self.im[k]);
            self.re[k] = pr * re - pi * im + self.gain[k] * x;
            let im = pr * im + pi * re;
            self.im[k] = im;
            out += im;
        }
        out
    }

    /// The most its output can be now (the sum of its modes' magnitudes).
    pub fn reach(&self) -> f32 {
        (0..self.len)
            .map(|k| (self.re[k] * self.re[k] + self.im[k] * self.im[k]).sqrt())
            .sum()
    }

    /// How many resonators it runs.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether it runs none.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}
