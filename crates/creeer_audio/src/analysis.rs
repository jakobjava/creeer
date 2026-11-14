//! Measuring a sound without ears: its spectrum's peaks (a Hann-windowed FFT), how fast it dies
//! away (Schroeder's backward integration, as rooms' reverberation is measured: ISO 3382), and
//! its level. The tests and the offline renderer's table use these.

use crate::filters::Biquad;
use std::f64::consts::PI;

/// The magnitude spectrum of `samples` (the first power of two of them, Hann-windowed): each bin's
/// frequency (Hz) and amplitude (as a sine's).
pub fn spectrum(samples: &[f32], rate: f32) -> Vec<(f64, f64)> {
    let n = samples.len().next_power_of_two()
        / if samples.len().is_power_of_two() {
            1
        } else {
            2
        };
    let n = n.max(2);
    let mut re: Vec<f64> = (0..n)
        .map(|k| {
            let w = 0.5 - 0.5 * (2.0 * PI * k as f64 / n as f64).cos();
            f64::from(samples.get(k).copied().unwrap_or(0.0)) * w
        })
        .collect();
    let mut im = vec![0.0; n];
    fft(&mut re, &mut im);
    // A sine of amplitude A shows as A·n/4 through a Hann window.
    let scale = 4.0 / n as f64;
    (0..n / 2)
        .map(|k| {
            (
                k as f64 * f64::from(rate) / n as f64,
                (re[k] * re[k] + im[k] * im[k]).sqrt() * scale,
            )
        })
        .collect()
}

/// An in-place radix-2 FFT.
fn fft(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let angle = -2.0 * PI / len as f64;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (s, c) = (angle * k as f64).sin_cos();
                let (a, b) = (start + k, start + k + len / 2);
                let (tr, ti) = (re[b] * c - im[b] * s, re[b] * s + im[b] * c);
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
            }
        }
        len <<= 1;
    }
}

/// The `count` strongest peaks of a spectrum (local maxima, refined between bins by a parabola
/// through the log magnitudes), strongest first: (Hz, amplitude).
pub fn peaks(spectrum: &[(f64, f64)], count: usize) -> Vec<(f64, f64)> {
    let mut found: Vec<(f64, f64)> = Vec::new();
    for k in 1..spectrum.len().saturating_sub(1) {
        let (a, b, c) = (spectrum[k - 1].1, spectrum[k].1, spectrum[k + 1].1);
        if b > a && b >= c && b > 0.0 {
            let (la, lb, lc) = (a.max(1e-30).ln(), b.ln(), c.max(1e-30).ln());
            let d = la - 2.0 * lb + lc;
            let shift = if d.abs() > 1e-12 {
                0.5 * (la - lc) / d
            } else {
                0.0
            };
            let bin = spectrum[1].0 - spectrum[0].0;
            found.push((
                spectrum[k].0 + shift * bin,
                (lb - 0.25 * (la - lc) * shift).exp(),
            ));
        }
    }
    found.sort_by(|x, y| y.1.total_cmp(&x.1));
    found.truncate(count);
    found
}

/// How long `samples` take to die away by 60 dB (s): Schroeder's energy decay curve, its slope
/// fitted from −5 to −25 dB (T20, as ISO 3382 does), or None if it never falls that far.
pub fn decay_time(samples: &[f32], rate: f32) -> Option<f64> {
    let mut energy: Vec<f64> = samples
        .iter()
        .map(|&s| f64::from(s) * f64::from(s))
        .collect();
    for k in (0..energy.len().saturating_sub(1)).rev() {
        energy[k] += energy[k + 1];
    }
    let total = *energy.first()?;
    if total <= 0.0 {
        return None;
    }
    let db = |k: usize| 10.0 * (energy[k] / total).max(1e-30).log10();
    let start = (0..energy.len()).find(|&k| db(k) <= -5.0)?;
    let end = (start..energy.len()).find(|&k| db(k) <= -25.0)?;
    let seconds = (end - start) as f64 / f64::from(rate);
    Some(seconds * 3.0)
}

/// How long a mode of `f` Hz in `samples` takes to die away by 60 dB (s): the sound through two
/// narrow band-passes about it (so the other modes' leak stays 60 dB down), then [`decay_time`].
pub fn mode_decay(samples: &[f32], rate: f32, f: f64) -> Option<f64> {
    #[allow(clippy::cast_possible_truncation)] // Hz
    let band = Biquad::bandpass(f as f32, 8.0, rate);
    let (mut a, mut b) = (band, band);
    let filtered: Vec<f32> = samples.iter().map(|&s| b.run(a.run(s))).collect();
    decay_time(&filtered, rate)
}

/// The largest magnitude in `samples`.
pub fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0, |m, s| m.max(s.abs()))
}

/// One channel of stereo frames (0 left, 1 right).
pub fn channel(frames: &[[f32; 2]], c: usize) -> Vec<f32> {
    frames.iter().map(|f| f[c]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sines_peak_is_found_at_its_frequency_and_amplitude() {
        let rate = 48_000.0;
        let s: Vec<f32> = (0..16384)
            .map(|k| {
                let t = k as f64 / f64::from(rate);
                #[allow(clippy::cast_possible_truncation)]
                let v = (0.5 * (2.0 * PI * 1234.5 * t).sin() + 0.1 * (2.0 * PI * 5000.0 * t).sin())
                    as f32;
                v
            })
            .collect();
        let p = peaks(&spectrum(&s, rate), 2);
        assert!((p[0].0 - 1234.5).abs() < 1.0, "{p:?}");
        assert!((p[0].1 - 0.5).abs() < 0.03, "{p:?}");
        assert!((p[1].0 - 5000.0).abs() < 1.0, "{p:?}");
    }

    #[test]
    fn an_exponential_decays_in_its_own_time() {
        // e^(−d·t) falls 60 dB in 6.91/d seconds.
        let (rate, d) = (48_000.0, 10.0);
        let s: Vec<f32> = (0..48_000)
            .map(|k| {
                let t = f64::from(k) / f64::from(rate);
                #[allow(clippy::cast_possible_truncation)]
                let v = ((-d * t).exp() * (2.0 * PI * 800.0 * t).sin()) as f32;
                v
            })
            .collect();
        let t60 = decay_time(&s, rate).unwrap();
        assert!((t60 - 6.908 / d).abs() < 0.01 * 6.908 / d, "{t60}");
    }
}
