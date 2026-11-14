//! The mix: never clipping, at most so many sounds, the quietest giving way, far sounds quieter
//! and later, the volumes obeyed, and its cost on the audio thread.

use super::*;
use crate::library::Library;
use crate::modes::Modes;
use crate::voice::{Click, Excite, VoiceSpec};

const RATE: f32 = 48_000.0;

fn settings() -> Settings {
    Library::base().settings()
}

/// A loud blow of 24 modes (as many as a sound keeps) `metres` ahead.
fn blow(metres: f64, pa: f32, seed: u64) -> VoiceSpec {
    let mut modes = Modes::default();
    for k in 0..24 {
        modes.push(
            150.0 * f64::from(k + 1) * 1.07,
            5.0 + f64::from(k),
            f64::from(pa) / 24.0,
        );
    }
    VoiceSpec {
        at: DVec3::new(0.0, 0.0, metres),
        bus: Bus::Effects,
        key: 0,
        modes,
        click: Click {
            gain: pa,
            contact: 1e-4,
            corner: 300.0,
            baffled: false,
        },
        excite: Excite::Blow,
        peak: 2.0 * pa,
        seed,
    }
}

#[test]
fn nothing_clips_at_the_voice_limit_and_no_more_play_than_it() {
    let s = settings();
    let mut mixer = Mixer::new(RATE, s);
    let mut out = vec![[0.0f32; 2]; 4800];
    // Twice the limit of very loud blows (130 dB at a metre), at once and again.
    for round in 0..3 {
        for k in 0..(2 * s.voices) {
            mixer.command(Command::Play(blow(
                1.0 + 0.1 * k as f64,
                90.0,
                (round * 100 + k) as u64,
            )));
        }
        mixer.render(&mut out);
        let most = out
            .iter()
            .fold(0.0f32, |m, f| m.max(f[0].abs()).max(f[1].abs()));
        assert!(most < 1.0, "round {round}: a sample at {most}");
        assert!(mixer.stats.playing <= s.voices);
    }
    assert!(mixer.stats.stolen + mixer.stats.culled >= 2 * s.voices as u64);
    assert!(mixer.stats.peak < 1.0);
}

#[test]
fn a_far_sound_is_quieter_duller_and_later_and_beyond_hearing_is_never_begun() {
    let mut mixer = Mixer::new(RATE, settings());
    // Something loud first, so the window is set by it.
    let near = blow(2.0, 2.0, 1);
    let far = blow(68.6, 2.0, 2);
    let first_sound = |spec: VoiceSpec| {
        // The window held still (its floor above both), so the two compare as heard.
        let fixed = Settings {
            floor: 10.0,
            ..settings()
        };
        let mut m = Mixer::new(RATE, fixed);
        m.command(Command::Play(spec));
        let mut out = vec![[0.0f32; 2]; 24_000];
        m.render(&mut out);
        let start = out
            .iter()
            .position(|f| f[0].abs() > 1e-6)
            .unwrap_or(usize::MAX);
        let peak = out.iter().fold(0.0f32, |p, f| p.max(f[0].abs()));
        (start, peak)
    };
    let (near_start, near_peak) = first_sound(near);
    let (far_start, far_peak) = first_sound(far);
    // 68.6 m: a fifth of a second later (9600 samples), within the block it was placed in.
    assert!(near_start < 400, "{near_start}");
    assert!((far_start as i64 - 9600).abs() < 400, "{far_start}");
    assert!(far_peak < near_peak / 10.0, "{near_peak} {far_peak}");
    // A whisper of a sound 300 m off while something loud plays near: not begun.
    mixer.command(Command::Play(blow(1.0, 50.0, 3)));
    mixer.command(Command::Play(blow(300.0, 1e-4, 4)));
    assert_eq!(mixer.stats.started, 1);
    assert_eq!(mixer.stats.culled, 1);
}

#[test]
fn the_volumes_are_obeyed_and_eased() {
    let mut mixer = Mixer::new(RATE, settings());
    let mut wind = crate::happenings::wind(&Library::base(), 1.7, 77).1;
    wind.seed = 5;
    mixer.command(Command::Play(wind));
    let mut out = vec![[0.0f32; 2]; 48_000];
    mixer.render(&mut out);
    let loud = out[24_000..].iter().map(|f| f[0] * f[0]).sum::<f32>();
    mixer.command(Command::Volumes(Volumes {
        master: 1.0,
        effects: 1.0,
        ambience: 0.0,
    }));
    mixer.render(&mut out);
    let quiet = out[24_000..].iter().map(|f| f[0] * f[0]).sum::<f32>();
    assert!(loud > 0.0 && quiet < 1e-6 * loud, "{loud} {quiet}");
}

#[test]
fn the_audio_threads_cost_at_the_voice_limit() {
    // Every voice taken by 24 modes and a click, a second of sound: the best of three (other
    // work shares the machine). The modes ring on (a slow decay), so none ends early.
    let s = settings();
    let mut best = f64::MAX;
    for _ in 0..3 {
        let mut mixer = Mixer::new(RATE, s);
        for k in 0..s.voices {
            let mut b = blow(1.0 + k as f64, 1.0, k as u64);
            let mut slow = Modes::default();
            for m in b.modes.as_slice() {
                slow.push(f64::from(m.freq), 0.1, f64::from(m.amp));
            }
            b.modes = slow;
            mixer.command(Command::Play(b));
        }
        let mut out = vec![[0.0f32; 2]; 48_000];
        let start = std::time::Instant::now();
        mixer.render(&mut out);
        best = best.min(start.elapsed().as_secs_f64());
        assert_eq!(mixer.stats.playing, s.voices);
    }
    eprintln!(
        "{} voices of 24 modes: {:.1} % of the audio thread",
        s.voices,
        100.0 * best
    );
    // Within half a core in this lightly optimised test build; the release build's figure is in
    // ADR-0037 (Cost).
    assert!(best < 0.5, "{best}");
}
