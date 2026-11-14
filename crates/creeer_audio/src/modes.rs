//! How a body rings when struck (game thread; van den Doel and Pai 1998): its modes' frequencies
//! from its shape and material, each one's decay from the loss factor, and how loud each is at a
//! metre from where it was struck, how hard, and how long the contact lasted (ADR-0037,
//! decision 2).
//!
//! - A slender body (longer than three times its thickness) is an Euler–Bernoulli bar along its
//!   longest axis, free at both ends: bending in both planes, fₙ = (βₙL)²/(2πL²)·c·κ (c = √(E/ρ),
//!   κ = √(I/A) its section's radius of gyration), and stretching, fₙ = n·c/(2L).
//! - A nail standing out of wood is a cantilever of its standing length (clamped where it goes
//!   in): its pitch rises as it is driven.
//! - A compact body is a ball of its volume: the torsional modes of a free sphere, kₛa the roots
//!   of x·j′ₙ(x) = jₙ(x) (Lamb 1882), kₛ = 2πf/√(G/ρ).

use crate::acoustic::{Acoustic, pulse_share, radiated};
use std::f64::consts::PI;

/// The most modes a sound keeps (the loudest).
pub const MAX_MODES: usize = 24;
/// The highest and lowest frequencies kept (Hz): the ear's range.
pub const HIGHEST: f64 = 20_000.0;
pub const LOWEST: f64 = 20.0;

/// One mode: its frequency (Hz), decay rate (1/s) and pressure amplitude at a metre (Pa).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Mode {
    pub freq: f32,
    pub decay: f32,
    pub amp: f32,
}

/// The modes of a sound, the loudest [`MAX_MODES`] kept.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Modes {
    list: [Mode; MAX_MODES],
    len: usize,
}

impl Modes {
    /// Adds `m` if it is audible: when full, it replaces the quietest if louder.
    #[allow(clippy::cast_possible_truncation)] // Hz, 1/s and Pa as f32
    pub fn push(&mut self, freq: f64, decay: f64, amp: f64) {
        if !(LOWEST..=HIGHEST).contains(&freq) || amp.is_nan() || amp <= 0.0 {
            return;
        }
        let mode = Mode {
            freq: freq as f32,
            decay: decay as f32,
            amp: amp as f32,
        };
        if self.len < MAX_MODES {
            self.list[self.len] = mode;
            self.len += 1;
        } else if let Some(q) =
            (0..MAX_MODES).min_by(|&a, &b| self.list[a].amp.total_cmp(&self.list[b].amp))
            && self.list[q].amp < mode.amp
        {
            self.list[q] = mode;
        }
    }

    /// Every mode of `other` added.
    pub fn extend(&mut self, other: &Self) {
        for m in other.as_slice() {
            self.push(f64::from(m.freq), f64::from(m.decay), f64::from(m.amp));
        }
    }

    /// Every amplitude times `k`.
    pub fn scale(&mut self, k: f64) {
        #[allow(clippy::cast_possible_truncation)] // a gain
        let k = k as f32;
        for m in &mut self.list[..self.len] {
            m.amp *= k;
        }
    }

    /// The modes.
    pub fn as_slice(&self) -> &[Mode] {
        &self.list[..self.len]
    }

    /// The most their sum can reach (Pa at a metre).
    pub fn peak(&self) -> f64 {
        self.as_slice().iter().map(|m| f64::from(m.amp)).sum()
    }
}

/// A body's shape as it rings (m).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// A bar along its local x: its length, its width (along y) and depth (along z), round
    /// (a log or a rod: width and depth its diameter) or not.
    Bar {
        length: f64,
        width: f64,
        depth: f64,
        round: bool,
    },
    /// A compact body, as a ball of this radius.
    Ball { radius: f64 },
}

impl Shape {
    /// The shape of a box of these half extents, its longest axis first: a bar if slender, else
    /// a ball of its volume. Returns it and the order of the box's axes it takes (local x, y, z).
    pub fn of_box(half: [f64; 3]) -> (Self, [usize; 3]) {
        let mut order = [0, 1, 2];
        order.sort_by(|&a, &b| half[b].total_cmp(&half[a]));
        let [l, w, d] = order.map(|k| 2.0 * half[k]);
        if l >= 3.0 * w {
            let bar = Self::Bar {
                length: l,
                width: w,
                depth: d,
                round: false,
            };
            (bar, order)
        } else {
            let radius = (3.0 * l * w * d / (4.0 * PI)).cbrt();
            (Self::Ball { radius }, order)
        }
    }

    /// Its volume (m³).
    pub fn volume(&self) -> f64 {
        match *self {
            Self::Bar {
                length,
                width,
                depth,
                round,
            } => {
                if round {
                    PI / 4.0 * width * depth * length
                } else {
                    width * depth * length
                }
            }
            Self::Ball { radius } => 4.0 / 3.0 * PI * radius.powi(3),
        }
    }

    /// The radius of a ball of its volume (m): how big it is to the air (its click).
    pub fn size(&self) -> f64 {
        (3.0 * self.volume() / (4.0 * PI)).cbrt()
    }
}

/// A blow on a body: where along it (0 to 1 along a bar), its impulse in the body's axes (N·s:
/// x along a bar), and how long the contact lasted (s).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blow {
    pub along: f64,
    pub impulse: [f64; 3],
    pub contact: f64,
}

/// βₙL of a free–free bar's bending modes (Blevins, Formulas for Natural Frequency and Mode
/// Shape, table 8-1); beyond them (n + 3/2)π.
const FREE_FREE: [f64; 7] = [
    4.730_040_7,
    7.853_204_6,
    10.995_607_8,
    14.137_165_5,
    17.278_759_6,
    20.420_352_2,
    23.561_944_9,
];
/// βₙL of a cantilever's (clamped–free); beyond them (n + 1/2)π.
const CLAMPED_FREE: [f64; 6] = [
    1.875_104_1,
    4.694_091_1,
    7.854_757_4,
    10.995_540_7,
    14.137_168_5,
    17.278_759_5,
];

fn free_free(n: usize) -> f64 {
    FREE_FREE.get(n).copied().unwrap_or((n as f64 + 1.5) * PI)
}

fn clamped_free(n: usize) -> f64 {
    CLAMPED_FREE
        .get(n)
        .copied()
        .unwrap_or((n as f64 + 0.5) * PI)
}

/// A free–free bar's mode of eigenvalue `bl` (βL) at `xi` along it (0 to 1): its shape scaled
/// to a mean square of 1 (±2 at the ends). Written so the large exponentials never cancel.
pub fn free_free_shape(bl: f64, xi: f64) -> f64 {
    let (s, c) = bl.sin_cos();
    let denominator = bl.sinh() - s;
    let sigma = (bl.cosh() - c) / denominator;
    let one_less = (c - s - (-bl).exp()) / denominator;
    let u = bl * xi.clamp(0.0, 1.0);
    0.5 * one_less * u.exp() + 0.5 * (1.0 + sigma) * (-u).exp() + u.cos() - sigma * u.sin()
}

/// A cantilever's mode of eigenvalue `bl` at `xi` from its clamped end (mean square 1; ±2 at
/// the free end).
pub fn cantilever_shape(bl: f64, xi: f64) -> f64 {
    let (s, c) = bl.sin_cos();
    let denominator = bl.sinh() + s;
    let sigma = (bl.cosh() + c) / denominator;
    let one_less = (s - c - (-bl).exp()) / denominator;
    let u = bl * xi.clamp(0.0, 1.0);
    0.5 * one_less * u.exp() + 0.5 * (1.0 + sigma) * (-u).exp() - u.cos() + sigma * u.sin()
}

/// The roots kₛa of x·j′ₙ(x) = jₙ(x) for n = 2…5 and the second for n = 2: a free sphere's
/// torsional modes, its lowest (Lamb 1882). Checked against the Bessel functions in the tests.
pub const SPHERE_TORSION: [f64; 5] = [
    2.501_132_6,
    3.864_699_8,
    5.094_615_6,
    6.265_767_8,
    7.136_008_8,
];

/// How the bars ring: the modes of a bar of `mass` kg struck by `blow`, each decaying `extra`
/// per second besides its material's loss (held against something).
pub fn bar(material: &Acoustic, shape: &Shape, mass: f64, blow: &Blow, extra: f64) -> Modes {
    let mut modes = Modes::default();
    let Shape::Bar {
        length: l,
        width: w,
        depth: d,
        round,
    } = *shape
    else {
        return ball(material, shape.size(), mass, blow, extra);
    };
    let c = material.bar_speed();
    let gyration = |h: f64| if round { h / 4.0 } else { h / 12f64.sqrt() };
    // Bending across the width (the faces of L × depth move) and across the depth.
    for (j, kappa, face) in [
        (blow.impulse[1], gyration(w), d),
        (blow.impulse[2], gyration(d), w),
    ] {
        for n in 0..12 {
            let bl = free_free(n);
            let f = bl * bl / (2.0 * PI * l * l) * c * kappa;
            if f > HIGHEST {
                break;
            }
            let v =
                (j * free_free_shape(bl, blow.along)).abs() / mass * pulse_share(f, blow.contact);
            modes.push(f, material.decay(f) + extra, radiated(f, v, l * face, face));
        }
    }
    // Stretching, rung by the blow along it; its ends radiate.
    for n in 1..5 {
        let f = n as f64 * c / (2.0 * l);
        let shape = std::f64::consts::SQRT_2 * (n as f64 * PI * blow.along).cos();
        let v = (blow.impulse[0] * shape).abs() / mass * pulse_share(f, blow.contact);
        modes.push(
            f,
            material.decay(f) + extra,
            radiated(f, v, 2.0 * w * d, w.min(d)),
        );
    }
    modes
}

/// How a compact body rings: a ball of `radius` m and `mass` kg (its torsional modes; the blow's
/// whole impulse taken to reach them).
pub fn ball(material: &Acoustic, radius: f64, mass: f64, blow: &Blow, extra: f64) -> Modes {
    let mut modes = Modes::default();
    let j = blow.impulse.iter().map(|x| x * x).sum::<f64>().sqrt();
    for x in SPHERE_TORSION {
        let f = x * material.shear_speed() / (2.0 * PI * radius);
        let v = j / mass * pulse_share(f, blow.contact);
        modes.push(
            f,
            material.decay(f) + extra,
            radiated(f, v, PI * radius * radius, 2.0 * radius),
        );
    }
    modes
}

/// A rod of `diameter` m standing `length` m out of what holds it (a nail), its free end struck
/// across with impulse `across` (N·s) no faster than `speed` (m/s), the contact `contact` s. Its
/// ringing leaks into its holder of loss factor `held` (wood), which damps it as the holder's own
/// modes are damped (an approximation: the clamp's loss is not measured).
pub fn cantilever(
    material: &Acoustic,
    (length, diameter, held): (f64, f64, f64),
    across: f64,
    speed: f64,
    contact: f64,
) -> Modes {
    let mut modes = Modes::default();
    let mass = material.density * PI / 4.0 * diameter * diameter * length;
    let kappa = diameter / 4.0;
    for n in 0..8 {
        let bl = clamped_free(n);
        let f = bl * bl / (2.0 * PI * length * length) * material.bar_speed() * kappa;
        if f > HIGHEST {
            break;
        }
        let tip = cantilever_shape(bl, 1.0).abs();
        let v = (across * tip / mass).min(speed) * pulse_share(f, contact);
        modes.push(
            f,
            material.decay(f) + PI * f * held,
            radiated(f, v, length * diameter, diameter),
        );
    }
    modes
}

/// A bar set ringing by `energy` J (a member snapping back as it breaks): its bending modes
/// across its thinner side (the way it bent) take the energy as a released bend would, the n-th
/// by 1/(βₙL)⁴.
pub fn released(material: &Acoustic, shape: &Shape, mass: f64, energy: f64) -> Modes {
    let mut modes = Modes::default();
    let Shape::Bar {
        length: l,
        width: w,
        depth: d,
        round,
    } = *shape
    else {
        return modes;
    };
    let (h, d) = (w.min(d), w.max(d));
    let kappa = if round { h / 4.0 } else { h / 12f64.sqrt() };
    let weights: Vec<f64> = (0..10).map(|n| free_free(n).powi(-4)).collect();
    let total: f64 = weights.iter().sum();
    for (n, share) in weights.iter().enumerate() {
        let bl = free_free(n);
        let f = bl * bl / (2.0 * PI * l * l) * material.bar_speed() * kappa;
        let v = (2.0 * energy * share / total / mass).sqrt();
        modes.push(f, material.decay(f), radiated(f, v, l * d, d));
    }
    modes
}

#[cfg(test)]
#[path = "modes_tests.rs"]
mod tests;
