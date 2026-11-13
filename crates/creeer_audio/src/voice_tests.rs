//! The synthesis against the physics, measured without ears: a struck bar's spectrum peaks at
//! beam theory's frequencies, each mode dies away as its material's loss factor says, the sound
//! grows with the blow, and the drives do what they should.

use super::*;
use crate::acoustic::Acoustic;
use crate::analysis::{decay_time, mode_decay, peak, peaks, spectrum};
use crate::modes::{Blow, Modes, Shape, bar};
use crate::offline::alone;
use std::f64::consts::PI;

const RATE: f32 = 48_000.0;

const STEEL: Acoustic = Acoustic {
    youngs: 210e9,
    shear: 80.8e9,
    density: 7850.0,
    loss: 3e-4,
};
const SPRUCE: Acoustic = Acoustic {
    youngs: 11e9,
    shear: 0.69e9,
    density: 420.0,
    loss: 0.008,
};

fn struck(material: &Acoustic, shape: Shape, j: f64, contact: f64) -> VoiceSpec {
    let mass = material.density * shape.volume();
    let blow = Blow {
        along: 0.05,
        impulse: [0.0, j, 0.0],
        contact,
    };
    let modes = bar(material, &shape, mass, &blow, 0.0);
    VoiceSpec {
        at: DVec3::ZERO,
        bus: Bus::Effects,
        key: 0,
        modes,
        click: Click::default(),
        excite: Excite::Blow,
        #[allow(clippy::cast_possible_truncation)]
        peak: modes.peak() as f32,
        seed: 1,
    }
}

/// A 1 m steel bar, 25 mm square: f₁ = 4.730²/(2π)·√(E/ρ)·h/√12 ≈ 132.9 Hz.
fn steel_bar() -> Shape {
    Shape::Bar {
        length: 1.0,
        width: 0.025,
        depth: 0.025,
        round: false,
    }
}

#[test]
fn a_struck_steel_bar_sounds_at_beam_theorys_frequencies() {
    let spec = struck(&STEEL, steel_bar(), 1.0, 5e-5);
    let sound = alone(spec, RATE, 2.0);
    // Two seconds of it: bins of 0.73 Hz (its first 65536 samples at 48 kHz).
    let found = peaks(&spectrum(&sound, RATE), 12);
    let c = (210e9f64 / 7850.0).sqrt();
    let kappa = 0.025 / 12f64.sqrt();
    let f1 = 4.730_040_7f64.powi(2) / (2.0 * PI) * c * kappa;
    // The first four bending modes, each within 0.2 % (a third of a bin at f₁).
    for (n, ratio) in [1.0, 2.756_5, 5.403_9, 8.933_0].iter().enumerate() {
        let want = f1 * ratio;
        let near = found
            .iter()
            .map(|p| p.0)
            .min_by(|a, b| (a - want).abs().total_cmp(&(b - want).abs()))
            .unwrap();
        assert!(
            (near - want).abs() < 0.002 * want,
            "mode {n}: {near:.1} Hz, beam theory {want:.1} Hz; peaks {found:?}"
        );
    }
}

#[test]
fn each_mode_dies_away_as_its_loss_factor_says() {
    // A spruce plank: T60 = 6.91/(π·f·η).
    let shape = Shape::Bar {
        length: 1.5,
        width: 0.15,
        depth: 0.05,
        round: false,
    };
    let spec = struck(&SPRUCE, shape, 2.0, 1e-4);
    let sound = alone(spec, RATE, 3.0);
    let modes = spec.modes;
    let mut checked = 0;
    for m in modes
        .as_slice()
        .iter()
        .filter(|m| m.freq > 150.0 && m.freq < 3000.0)
        .take(3)
    {
        let f = f64::from(m.freq);
        // Only a mode standing clear of its neighbours (a band-pass of Q 8 spans f/8).
        let clear = modes
            .as_slice()
            .iter()
            .all(|o| o == m || (f64::from(o.freq) - f).abs() > f / 4.0);
        if !clear {
            continue;
        }
        let want = 6.908 / (PI * f * 0.008);
        let got = mode_decay(&sound, RATE, f).unwrap();
        assert!(
            (got - want).abs() < 0.05 * want,
            "{f:.0} Hz: T60 {got:.3} s, the loss factor's {want:.3} s"
        );
        checked += 1;
    }
    assert!(checked >= 2, "too few clear modes: {modes:?}");
    // A metal of loss factor 0.001 rings eight times as long as the spruce's 0.008.
    let metal = Acoustic {
        loss: 1e-3,
        ..STEEL
    };
    let ringing = alone(struck(&metal, steel_bar(), 1.0, 5e-5), RATE, 20.0);
    let wood = alone(struck(&SPRUCE, steel_bar(), 1.0, 5e-5), RATE, 6.0);
    let ratio = decay_time(&ringing, RATE).unwrap() / decay_time(&wood, RATE).unwrap();
    assert!((ratio - 8.0).abs() < 0.15 * 8.0, "{ratio}");
}

#[test]
fn a_harder_blow_sounds_louder_by_as_much() {
    let level = |j: f64| {
        f64::from(peak(&alone(
            struck(&SPRUCE, steel_bar(), j, 1e-4),
            RATE,
            0.5,
        )))
    };
    let (one, two, ten) = (level(1.0), level(2.0), level(10.0));
    // Twice the impulse, twice the pressure (+6 dB); ten times, +20 dB.
    assert!(
        (20.0 * (two / one).log10() - 6.02).abs() < 0.1,
        "{one} {two}"
    );
    assert!(
        (20.0 * (ten / one).log10() - 20.0).abs() < 0.1,
        "{one} {ten}"
    );
}

#[test]
fn a_blows_click_is_brighter_the_shorter_its_contact() {
    let click = |contact: f32| {
        let spec = VoiceSpec {
            at: DVec3::ZERO,
            bus: Bus::Effects,
            key: 0,
            modes: Modes::default(),
            click: Click {
                gain: 1.0,
                contact,
                corner: 500.0,
            },
            excite: Excite::Blow,
            peak: 1.0,
            seed: 1,
        };
        let s = spectrum(&alone(spec, RATE, 0.1), RATE);
        let total: f64 = s.iter().map(|b| b.1 * b.1).sum();
        s.iter().map(|b| b.0 * b.1 * b.1).sum::<f64>() / total
    };
    // Stone on stone (tens of µs) against wood on soil (milliseconds).
    let (hard, soft) = (click(3e-5), click(3e-3));
    assert!(
        hard > 4.0 * soft,
        "spectral centroids {hard:.0} Hz and {soft:.0} Hz"
    );
}

fn grains(count: f32, seconds: f32, seed: u64) -> VoiceSpec {
    let mut modes = Modes::default();
    modes.push(800.0, 30.0, 1.0);
    VoiceSpec {
        at: DVec3::ZERO,
        bus: Bus::Effects,
        key: 0,
        modes,
        click: Click::default(),
        excite: Excite::Grains {
            count,
            seconds,
            exponent: 1.4,
            shape: Envelope::Burst,
        },
        peak: 1.0,
        seed,
    }
}

#[test]
fn a_bursts_grains_carry_one_blows_energy_whatever_their_count() {
    // The energy of the modes driven by a burst, averaged over seeds, is near a single blow's.
    let energy = |s: &[f32]| s.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>();
    let mut blow = grains(1.0, 0.1, 0);
    blow.excite = Excite::Blow;
    let one = energy(&alone(blow, RATE, 1.0));
    for count in [30.0, 300.0] {
        let mean: f64 = (0..40)
            .map(|seed| energy(&alone(grains(count, 0.1, seed), RATE, 1.0)))
            .sum::<f64>()
            / 40.0;
        // Random phases add energies; the power law's spread allows a factor of two.
        assert!(
            mean > 0.5 * one && mean < 2.0 * one,
            "{count} grains: {mean} vs one blow {one}"
        );
    }
}

fn creak(rate: f32, seed: u64) -> usize {
    let mut modes = Modes::default();
    modes.push(300.0, 20.0, 1.0);
    let spec = VoiceSpec {
        at: DVec3::ZERO,
        bus: Bus::Effects,
        key: 9,
        modes,
        click: Click::default(),
        excite: Excite::Creak { rate, level: 1.0 },
        peak: 1.0,
        seed,
    };
    let mut voice = Voice::new(spec, RATE, 0);
    let mut slips = 0;
    let mut last = 0.0f32;
    for _ in 0..(10 * 48_000) {
        let _ = voice.sample();
        // A slip shows as the drive's level jumping in: counted from the resonator's reach.
        let reach = voice.bank.reach();
        if reach > last + 0.2 {
            slips += 1;
        }
        last = reach;
    }
    slips
}

#[test]
fn a_member_creaks_more_the_more_it_is_loaded() {
    let none = creak(0.0, 1);
    let some = creak(0.5, 1);
    let more = creak(3.0, 1);
    assert_eq!(none, 0);
    assert!(more > 2 * some && some > 0, "slips {some} then {more}");
}

#[test]
fn a_continuous_sound_fades_once_stopped_and_ends() {
    let mut modes = Modes::default();
    modes.push(400.0, 30.0, 1.0);
    let spec = VoiceSpec {
        at: DVec3::ZERO,
        bus: Bus::Effects,
        key: 3,
        modes,
        click: Click::default(),
        excite: Excite::Rub {
            force: 5.0,
            corner: 2000.0,
        },
        peak: 1.0,
        seed: 2,
    };
    let mut voice = Voice::new(spec, RATE, 0);
    for _ in 0..48_000 {
        voice.sample();
    }
    assert!(!voice.done());
    voice.stopping = true;
    let mut ended = None;
    for k in 0..(3 * 48_000) {
        voice.sample();
        if voice.done() {
            ended = Some(k);
            break;
        }
    }
    assert!(ended.is_some_and(|k| k < 48_000), "{ended:?}");
}
