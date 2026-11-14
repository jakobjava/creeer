//! The audio thread's filters: two-pole sections from Robert Bristow-Johnson's "Audio EQ
//! Cookbook" (transposed direct form II), and a one-pole low-pass.

use std::f32::consts::PI;

/// A two-pole filter section.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    z: [f32; 2],
}

/// The corner kept below the Nyquist frequency (Hz at `rate` samples a second).
fn corner(f: f32, rate: f32) -> f32 {
    f.clamp(1.0, 0.45 * rate)
}

impl Biquad {
    fn from(b: [f32; 3], a: [f32; 3]) -> Self {
        Self {
            b: b.map(|x| x / a[0]),
            a: [a[1] / a[0], a[2] / a[0]],
            z: [0.0; 2],
        }
    }

    /// A low-pass at `f` Hz of quality `q` (0.707: no peak).
    pub fn lowpass(f: f32, q: f32, rate: f32) -> Self {
        let w = 2.0 * PI * corner(f, rate) / rate;
        let (s, c) = w.sin_cos();
        let alpha = s / (2.0 * q);
        let k = (1.0 - c) / 2.0;
        Self::from([k, 1.0 - c, k], [1.0 + alpha, -2.0 * c, 1.0 - alpha])
    }

    /// A high-pass at `f` Hz of quality `q`.
    pub fn highpass(f: f32, q: f32, rate: f32) -> Self {
        let w = 2.0 * PI * corner(f, rate) / rate;
        let (s, c) = w.sin_cos();
        let alpha = s / (2.0 * q);
        let k = (1.0 + c) / 2.0;
        Self::from([k, -1.0 - c, k], [1.0 + alpha, -2.0 * c, 1.0 - alpha])
    }

    /// A first-order high-pass at `f` Hz (6 dB an octave below it; bilinear).
    pub fn highpass1(f: f32, rate: f32) -> Self {
        let k = (PI * corner(f, rate) / rate).tan();
        Self::from([1.0, -1.0, 0.0], [1.0 + k, k - 1.0, 0.0])
    }

    /// A band-pass at `f` Hz of quality `q`, its peak at unit gain.
    pub fn bandpass(f: f32, q: f32, rate: f32) -> Self {
        let w = 2.0 * PI * corner(f, rate) / rate;
        let (s, c) = w.sin_cos();
        let alpha = s / (2.0 * q);
        Self::from([alpha, 0.0, -alpha], [1.0 + alpha, -2.0 * c, 1.0 - alpha])
    }

    /// Its coefficients set to `other`'s, its state kept (a filter retuned while it runs).
    pub fn retune(&mut self, other: &Self) {
        self.b = other.b;
        self.a = other.a;
    }

    /// One sample through it.
    #[inline]
    pub fn run(&mut self, x: f32) -> f32 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

/// A one-pole low-pass.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OnePole {
    k: f32,
    y: f32,
}

impl OnePole {
    /// The coefficient of a corner at `f` Hz (its −3 dB point).
    pub fn coefficient(f: f32, rate: f32) -> f32 {
        1.0 - (-2.0 * PI * corner(f, rate) / rate).exp()
    }

    /// Sets its corner by coefficient (from [`OnePole::coefficient`]).
    pub fn set(&mut self, k: f32) {
        self.k = k;
    }

    /// One sample through it.
    #[inline]
    pub fn run(&mut self, x: f32) -> f32 {
        self.y += self.k * (x - self.y);
        self.y
    }
}

#[cfg(test)]
mod tests {
    use super::{Biquad, OnePole};
    use std::f32::consts::PI;

    /// The steady amplitude of a sine of `f` Hz through `filter`.
    fn gain(mut filter: impl FnMut(f32) -> f32, f: f32, rate: f32) -> f32 {
        let n = 48_000;
        let mut peak = 0.0f32;
        for k in 0..n {
            #[allow(clippy::cast_precision_loss)] // under 2^24
            let t = k as f32 / rate;
            let y = filter((2.0 * PI * f * t).sin());
            if k > n / 2 {
                peak = peak.max(y.abs());
            }
        }
        peak
    }

    #[test]
    fn the_cookbook_sections_pass_and_stop_where_they_should() {
        let rate = 48_000.0;
        let mut lp = Biquad::lowpass(1000.0, 0.707, rate);
        assert!((gain(|x| lp.run(x), 100.0, rate) - 1.0).abs() < 0.01);
        let mut lp = Biquad::lowpass(1000.0, 0.707, rate);
        assert!((gain(|x| lp.run(x), 1000.0, rate) - 0.707).abs() < 0.01);
        let mut lp = Biquad::lowpass(1000.0, 0.707, rate);
        assert!(gain(|x| lp.run(x), 10_000.0, rate) < 0.015);
        let mut hp = Biquad::highpass(1000.0, 0.707, rate);
        assert!(gain(|x| hp.run(x), 100.0, rate) < 0.015);
        let mut bp = Biquad::bandpass(2000.0, 2.0, rate);
        assert!((gain(|x| bp.run(x), 2000.0, rate) - 1.0).abs() < 0.01);
        let mut one = OnePole::default();
        one.set(OnePole::coefficient(1000.0, rate));
        let g = gain(|x| one.run(x), 1000.0, rate);
        assert!((g - 0.707).abs() < 0.03, "{g}");
    }
}
