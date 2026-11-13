//! More of what happens, as sounds (game thread; ADR-0037 decision 5): a member breaking and
//! creaking before it does, footsteps, things rolling and sliding, the wind.

use crate::acoustic::{click, contact_modulus, contact_time, pressure};
use crate::events::Body;
use crate::library::{Ground, Library, Matter};
use crate::modes::{Modes, Shape, released};
use crate::voice::{Bus, Click, Envelope, Excite, VoiceSpec};
use creeer_math::glam::DVec3;
use std::f64::consts::PI;

/// The share of a member's volume whose strain energy a bending break releases: a beam loaded to
/// its strength at one point holds σ²/(2E) in its outermost fibre at the worst section, a ninth of
/// that over its whole volume (a rectangular section under a triangle of moment).
const BENT: f64 = 1.0 / 9.0;

#[allow(clippy::cast_possible_truncation)] // Pa, s and Hz as f32
fn voice(at: DVec3, modes: Modes, click: Click, excite: Excite, seed: u64) -> VoiceSpec {
    VoiceSpec {
        at,
        bus: Bus::Effects,
        key: 0,
        modes,
        click,
        excite,
        peak: (modes.peak() + f64::from(click.gain)) as f32,
        seed,
    }
}

/// Member `body` (a bar) broke at `share` along it: a burst of micro-fractures (its material's
/// crack) through the two pieces' modes, set ringing by the share of the strain energy it held at
/// its strength that its material lets become vibration.
#[allow(clippy::cast_possible_truncation)] // s and counts as f32
pub fn crack(body: &Body<'_>, share: f64, seed: u64) -> VoiceSpec {
    let m = body.matter;
    let a = m.acoustic;
    let energy =
        m.strength * m.strength / (2.0 * a.youngs) * body.shape.volume() * BENT * m.crack.vibration;
    let mut modes = Modes::default();
    let mut gain = 0.0;
    if let Shape::Bar {
        length,
        width,
        depth,
        round,
    } = body.shape
    {
        for piece in [share, 1.0 - share] {
            let shape = Shape::Bar {
                length: length * piece.clamp(0.05, 0.95),
                width,
                depth,
                round,
            };
            let mass = a.density * shape.volume();
            modes.extend(&released(&a, &shape, mass, 0.5 * energy));
            // The pieces snap back at the speed their share of the energy gives them.
            gain += click(shape.size()).0 * (energy / mass).sqrt();
        }
    }
    let corner = click(0.5 * body.curvature()).1;
    let click = Click {
        gain: gain as f32,
        contact: 1e-4,
        corner: corner as f32,
    };
    let excite = Excite::Grains {
        count: m.crack.grains as f32,
        seconds: m.crack.seconds as f32,
        exponent: m.crack.exponent as f32,
        shape: Envelope::Burst,
    };
    let length = if let Shape::Bar { length, .. } = body.shape {
        length
    } else {
        0.0
    };
    let at = body.centre + body.turn * DVec3::X * ((share - 0.5) * length);
    voice(at, modes, click, excite, seed)
}

/// Member `body` loaded to `utilisation` of its strength (σ/f): its creaking, if it creaks yet
/// (timber's acoustic emission: none below its onset, then a rate rising as the cube of the way
/// to breaking, each slip's energy as its square). `key` names it, to change or stop it.
#[allow(clippy::cast_possible_truncation)] // rates and N·s as f32
pub fn creak(body: &Body<'_>, utilisation: f64, key: u64, seed: u64) -> Option<VoiceSpec> {
    let c = body.matter.creak?;
    let x = ((utilisation - c.onset) / (1.0 - c.onset)).clamp(0.0, 1.0);
    if x <= 0.0 {
        return None;
    }
    // A slip of energy e into the modes: the impulse √(2·m·e) (each mode set for 1 N·s).
    let slip = (2.0 * body.mass * c.energy * x * x).sqrt();
    // Struck where it is most bent, across, and as a slip is slow: half a millisecond.
    let modes = body.modes(body.centre, body.turn * DVec3::Y, 5e-4);
    let excite = Excite::Creak {
        rate: (c.rate * x * x * x) as f32,
        level: slip as f32,
    };
    let mut spec = voice(body.centre, modes, Click::default(), excite, seed);
    spec.key = key;
    spec.peak = (modes.peak() * slip) as f32;
    Some(spec)
}

/// What a foot comes down on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Underfoot<'a> {
    Ground(&'a Ground),
    /// A body (a log of a bridge), its material, shape and mass.
    Body(Body<'a>),
}

/// The heel's curvature where it strikes (m; an estimate of a boot's heel).
const HEEL: f64 = 0.04;
/// When the toe pushes off after the heel strikes (s; walking: about half the stance).
const TOE: f32 = 0.3;

/// A foot coming down at `at` (pressing along `down`) on `under`, its `scale` the step's strength
/// (1 a walking adult's): the heel's thud (through the body's modes if on one) and, on loose
/// ground, the grains under it crunching (Cook's PhISEM). Up to two sounds.
#[allow(clippy::cast_possible_truncation)] // s, Hz and counts as f32
pub fn footstep(
    lib: &Library,
    under: &Underfoot<'_>,
    (at, down): (DVec3, DVec3),
    scale: f64,
    seed: u64,
) -> [Option<VoiceSpec>; 2] {
    let heel = lib.hearing.heel;
    let sole = (1.0 - 0.45 * 0.45) / heel.sole;
    let speed = heel.speed * scale;
    let j = heel.mass * speed;
    let (ground, modes, compliance) = match under {
        Underfoot::Ground(g) => (Some(*g), Modes::default(), (1.0 - 0.09) / g.modulus),
        Underfoot::Body(b) => (None, Modes::default(), b.matter.acoustic.compliance()),
    };
    let tau = contact_time(heel.mass, HEEL, contact_modulus(sole, compliance), speed);
    let modes = match under {
        Underfoot::Body(b) => b.modes(at, down * j, tau),
        Underfoot::Ground(_) => modes,
    };
    let (gain, corner) = click(HEEL);
    let thud = voice(
        at,
        modes,
        Click {
            gain: (gain * speed) as f32,
            contact: tau as f32,
            corner: corner as f32,
        },
        Excite::Blow,
        seed,
    );
    let grains = ground.filter(|g| g.grains >= 1.0).map(|g| {
        // Each grain a pebble of its size, moved at its share of the heel's speed.
        let a = 0.5 * g.grain;
        let mass = 2650.0 * 4.0 / 3.0 * PI * a * a * a;
        let v = g.shift * speed;
        let modulus = contact_modulus(
            (1.0 - 0.09) / g.grain_modulus,
            (1.0 - 0.09) / g.grain_modulus,
        );
        let tau = contact_time(0.5 * mass, 0.5 * a, modulus, v);
        let (gain, corner) = click(a);
        let count = g.grains * scale;
        voice(
            at,
            Modes::default(),
            Click {
                gain: (gain * v * count.sqrt()) as f32,
                contact: tau as f32,
                corner: corner as f32,
            },
            Excite::Grains {
                count: count as f32,
                seconds: TOE + 0.2,
                exponent: 1.5,
                shape: Envelope::Step { toe: TOE },
            },
            seed ^ 0x5eed,
        )
    });
    [Some(thud), grains]
}

/// Body `body` sliding (or rolling) along ground at `speed` m/s, pressed on it by its weight: a
/// rough contact read at the speed (FoleyAutomatic). Rolling is far gentler, and duller: the
/// contact patch smooths what it rolls over. `key` names it.
#[allow(clippy::cast_possible_truncation)] // N and Hz as f32
pub fn rub(
    body: &Body<'_>,
    at: DVec3,
    (speed, rolling): (f64, bool),
    key: u64,
    seed: u64,
) -> VoiceSpec {
    // The friction force's fluctuation (a share of μ·m·g with μ ½) and the roughness's wavelength.
    let (share, wavelength) = if rolling { (0.02, 0.01) } else { (0.15, 0.001) };
    let force = share * 0.5 * body.mass * 9.81;
    let corner = (speed / wavelength).clamp(30.0, 15_000.0);
    let modes = body.modes(at, body.turn * DVec3::Y, 1e-3);
    let excite = Excite::Rub {
        force: force as f32,
        corner: corner as f32,
    };
    let mut spec = voice(at, modes, Click::default(), excite, seed);
    spec.key = key;
    spec.peak = (modes.peak() * force * 0.01) as f32;
    spec
}

/// The wind heard `height` m above the ground: its speed by the log profile from the mean at
/// 10 m, u(z) = u₁₀·ln(z/z₀)/ln(10/z₀), and its sound (`hearing.ron`), as the speed cubed.
/// Returns the speed (m/s) and its sound (key `key`).
#[allow(clippy::cast_possible_truncation)] // m/s and Pa as f32
pub fn wind(lib: &Library, height: f64, key: u64) -> (f64, VoiceSpec) {
    let w = lib.hearing.wind;
    let z = height.max(2.0 * w.roughness);
    let speed = w.speed * (z / w.roughness).ln() / (10.0 / w.roughness).ln();
    let level = pressure(w.level) * (speed / 5.0).powi(3);
    let spec = VoiceSpec {
        at: DVec3::ZERO,
        bus: Bus::Ambience,
        key,
        modes: Modes::default(),
        click: Click::default(),
        excite: Excite::Wind {
            speed: speed as f32,
            level: level as f32,
        },
        peak: level as f32,
        seed: key,
    };
    (speed, spec)
}

/// The material a body the client is told only its Young's modulus and density of is made of.
pub fn matter_of(lib: &Library, youngs: f64, density: f64) -> &Matter {
    lib.nearest(youngs, density)
}
