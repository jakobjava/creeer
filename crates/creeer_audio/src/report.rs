//! The offline renderer's output (ADR-0037, decision 7): each reference scene through the mixer
//! to a WAV file, and a table of what each should sound like beside what was measured of it, for
//! the user to read while listening.

use crate::acoustic::spl;
use crate::analysis::{decay_time, peak, peaks, spectrum};
use crate::library::Library;
use crate::offline::{alone, render};
use crate::scenes::Scene;
use std::fmt::Write as _;
use std::path::Path;

/// The rate the files are written at (samples a second).
pub const RATE: u32 = 48_000;

/// What was measured of a scene.
#[derive(Clone, Debug, PartialEq)]
pub struct Measured {
    pub name: String,
    /// Its measured sound's strongest peaks (Hz), its decay (s) and its peak at a metre (dB SPL).
    pub peaks: Vec<f64>,
    pub decay: Option<f64>,
    pub level: Option<f64>,
    /// The file's loudest sample (dB of full scale), and sounds begun and not begun.
    pub file_peak: f64,
    pub started: u64,
    pub culled: u64,
}

/// Measures `scene` and renders it (the frames returned).
#[allow(clippy::cast_precision_loss)] // the rate
pub fn measure(lib: &Library, scene: &Scene) -> (Measured, Vec<[f32; 2]>) {
    let rate = RATE as f32;
    let (frames, stats) = render(lib.settings(), rate, scene.seconds, &scene.commands);
    let file = frames
        .iter()
        .fold(0.0f32, |m, f| m.max(f[0].abs()).max(f[1].abs()));
    let (found, decay, level) = match scene.measured {
        Some(spec) => {
            let sound = alone(spec, rate, scene.seconds.min(4.0));
            let mut found: Vec<f64> = peaks(&spectrum(&sound, rate), 4)
                .iter()
                .map(|p| p.0)
                .collect();
            found.sort_by(f64::total_cmp);
            (
                found,
                decay_time(&sound, rate),
                Some(spl(f64::from(peak(&sound)))),
            )
        }
        None => (Vec::new(), None, None),
    };
    let measured = Measured {
        name: scene.name.to_owned(),
        peaks: found,
        decay,
        level,
        file_peak: 20.0 * f64::from(file.max(1e-9)).log10(),
        started: stats.started,
        culled: stats.culled,
    };
    (measured, frames)
}

/// Every scene of `scenes` written to `dir` as `<name>.wav`, and the table (Markdown) of them.
///
/// # Errors
/// If a file cannot be written.
pub fn write_all(lib: &Library, scenes: &[Scene], dir: &Path) -> Result<String, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut table = String::from(
        "| File | What it should sound like | Lowest mode, beam theory (Hz) | Measured peaks (Hz) | Decay T60 (s) | Peak at 1 m (dB SPL) | File peak (dBFS) | Sounds begun / not |\n|---|---|---|---|---|---|---|---|\n",
    );
    for scene in scenes {
        let (m, frames) = measure(lib, scene);
        let path = dir.join(format!("{}.wav", scene.name));
        crate::wav::write(&path, &frames, RATE).map_err(|e| format!("{}: {e}", path.display()))?;
        let peaks: Vec<String> = m.peaks.iter().map(|f| format!("{f:.0}")).collect();
        let _ = writeln!(
            table,
            "| {}.wav | {} | {} | {} | {} | {} | {:.1} | {} / {} |",
            scene.name,
            scene.what,
            scene.expected.map_or("–".into(), |f| format!("{f:.1}")),
            if peaks.is_empty() {
                "–".into()
            } else {
                peaks.join(", ")
            },
            m.decay.map_or("–".into(), |t| format!("{t:.2}")),
            m.level.map_or("–".into(), |l| format!("{l:.0}")),
            m.file_peak,
            m.started,
            m.culled,
        );
    }
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every reference scene renders, is heard, never clips, and the struck bodies' lowest peaks
    /// stand where beam theory puts them.
    #[test]
    fn every_reference_scene_is_heard_and_none_clips() {
        let lib = Library::base();
        for scene in crate::scenes::all(&lib) {
            let (m, frames) = measure(&lib, &scene);
            eprintln!("{m:?}");
            assert!(m.started > 0, "{}: nothing begun", m.name);
            assert!(
                m.file_peak < 0.0,
                "{}: peak {:.1} dBFS",
                m.name,
                m.file_peak
            );
            assert!(frames.iter().all(|f| f[0].is_finite() && f[1].is_finite()));
            if let (Some(want), Some(lowest)) = (scene.expected, m.peaks.first()) {
                // The lowest of the four strongest is the fundamental or an overtone of it.
                let ratio = lowest / want;
                let near = [1.0, 2.756, 5.404]
                    .iter()
                    .any(|r| (ratio - r).abs() < 0.01 * r);
                assert!(
                    near,
                    "{}: lowest strong peak {lowest:.1} Hz, beam theory's f₁ {want:.1} Hz",
                    m.name
                );
            }
        }
    }
}
