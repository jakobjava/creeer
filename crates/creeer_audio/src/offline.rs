//! Rendering without a device (ADR-0037, decision 7): a recording of commands, each at its time,
//! through the same mixer the device plays, to stereo frames; or one sound alone, as the pressure
//! it makes a metre away (Pa), to measure.

use crate::mixer::{Command, Mixer, Settings, Stats};
use crate::output::Recording;
use crate::voice::{Voice, VoiceSpec};

/// `commands` (each at its time, s) rendered for `seconds` at `rate` samples a second through a
/// mixer of `settings`: the frames and what the mixer did.
pub fn render(
    settings: Settings,
    rate: f32,
    seconds: f64,
    commands: &Recording,
) -> (Vec<[f32; 2]>, Stats) {
    let mut mixer = Mixer::new(rate, settings);
    let total = samples(seconds, rate);
    let mut out = vec![[0.0f32; 2]; total];
    let mut sorted: Vec<&(f64, Command)> = commands.iter().collect();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut done = 0;
    for (at, command) in sorted {
        let k = samples(*at, rate).min(total);
        if k > done {
            mixer.render(&mut out[done..k]);
            done = k;
        }
        mixer.command(*command);
    }
    mixer.render(&mut out[done..]);
    (out, mixer.stats)
}

/// One sound alone, for `seconds`: the pressure it makes a metre away (Pa), sample by sample.
pub fn alone(spec: VoiceSpec, rate: f32, seconds: f64) -> Vec<f32> {
    let mut voice = Voice::new(spec, rate, 0);
    (0..samples(seconds, rate))
        .map(|_| voice.sample())
        .collect()
}

/// Samples in `seconds` at `rate`.
pub fn samples(seconds: f64, rate: f32) -> usize {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // short renders
    let n = (seconds.max(0.0) * f64::from(rate)).round() as usize;
    n
}
