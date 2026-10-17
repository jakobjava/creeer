//! The per-link and per-node work of an island's Newton iteration, element by element (so
//! the same bits whether a large island's elements are done in parallel or not).

use crate::convection::coefficient;
use crate::network::{Link, STEFAN_BOLTZMANN};
use rayon::prelude::*;

/// Islands of at least this many nodes do their per-element work in parallel.
pub const PARALLEL_FROM: usize = 2048;
/// Elements per parallel task at least.
pub const CHUNK: usize = 512;

/// A conduction link's conductivity is re-evaluated once its mean temperature has moved this
/// far (K) from where it was: a metal's or concrete's k changes by at most ~0.1 %/K.
const CONDUCTION_DRIFT: f64 = 0.5;
/// A convection coefficient is re-evaluated once the film temperature has moved this far (K)
/// (air's properties move h by ~0.3 %/K) ...
const FILM_DRIFT: f64 = 1.0;
/// ... or the temperature difference by this fraction of itself (h goes as ΔT^¼ to ΔT^⅓:
/// under 0.7 %) plus [`DIFFERENCE_FLOOR`]. Both far inside the correlations' own ±10–20 %.
const DIFFERENCE_DRIFT: f64 = 0.02;
/// Kelvin: below it a difference carries too little heat for its coefficient to matter.
const DIFFERENCE_FLOOR: f64 = 0.05;

/// Runs `f` on every element, in parallel if `parallel`.
pub fn each<T: Send>(items: &mut [T], parallel: bool, f: impl Fn(usize, &mut T) + Sync + Send) {
    if parallel {
        items
            .par_iter_mut()
            .enumerate()
            .with_min_len(CHUNK)
            .for_each(|(i, x)| f(i, x));
    } else {
        for (i, x) in items.iter_mut().enumerate() {
            f(i, x);
        }
    }
}

/// Runs `f` on every element, in parallel if `parallel`, and returns the largest of each of
/// the pairs it returns (the same either way: max is exact).
pub fn each_most<T: Send>(
    items: &mut [T],
    parallel: bool,
    f: impl Fn(usize, &mut T) -> (f64, f64) + Sync + Send,
) -> (f64, f64) {
    let most = |(a, b): (f64, f64), (c, d): (f64, f64)| (a.max(c), b.max(d));
    if parallel {
        items
            .par_iter_mut()
            .enumerate()
            .with_min_len(CHUNK)
            .map(|(i, x)| f(i, x))
            .reduce(|| (0.0, 0.0), most)
    } else {
        items
            .iter_mut()
            .enumerate()
            .map(|(i, x)| f(i, x))
            .fold((0.0, 0.0), most)
    }
}

/// Runs `f` on every pair of elements of `a` and `b`, in parallel if `parallel`.
pub fn each2<A: Send, B: Send>(
    a: &mut [A],
    b: &mut [B],
    parallel: bool,
    f: impl Fn(usize, &mut A, &mut B) + Sync + Send,
) {
    if parallel {
        a.par_iter_mut()
            .zip(b)
            .enumerate()
            .with_min_len(CHUNK)
            .for_each(|(i, (x, y))| f(i, x, y));
    } else {
        for (i, (x, y)) in a.iter_mut().zip(b).enumerate() {
            f(i, x, y);
        }
    }
}

/// How far `a` is from `b`, relative to the larger (a NaN `b`, never factorised, is far).
#[allow(clippy::float_cmp)] // the very same value: no change at all
pub fn relative(a: f64, b: f64) -> f64 {
    if a == b {
        0.0
    } else if b.is_nan() {
        f64::INFINITY
    } else {
        (a - b).abs() / a.abs().max(b.abs())
    }
}

/// What an iteration reads and writes of a link (kept small: an island's links are many, and
/// each iteration streams through them all).
#[derive(Clone, Copy, Debug)]
pub struct Hot {
    /// Its lagged coefficient (W/K; unused by radiation).
    pub g: f64,
    /// Its flow from its first node to its second (W).
    pub flow: f64,
    /// ∂flow/∂(its first node's temperature), and the symmetric coupling between its nodes:
    /// the lagged coefficient, except for radiation.
    pub da: f64,
    pub coupling: f64,
    /// (da, coupling) in the factorisation, and how far they are from it now (relative).
    pub factored: (f64, f64),
    pub drift: f64,
}

impl Default for Hot {
    fn default() -> Self {
        Self {
            g: 0.0,
            flow: 0.0,
            da: 0.0,
            coupling: 0.0,
            factored: (f64::NAN, f64::NAN),
            drift: f64::INFINITY,
        }
    }
}

/// The temperatures a link's coefficient was evaluated at (never, at first).
#[derive(Clone, Copy, Debug)]
pub struct Cold(pub f64, pub f64);

impl Default for Cold {
    fn default() -> Self {
        Self(f64::NAN, f64::NAN)
    }
}

/// Re-evaluates a link's lagged coefficient at the step's start temperatures `ta`, `tb` if
/// they moved far enough from where it last was (or it never was).
pub fn refresh(hot: &mut Hot, cold: &mut Cold, link: &Link, ta: f64, tb: f64, gravity: f64) {
    let Cold(a0, b0) = *cold;
    let fresh = match *link {
        Link::Conductance { conductance, .. } => {
            if !a0.is_nan() {
                return;
            }
            conductance
        }
        Link::Conduction {
            ref conductivity,
            area,
            length,
            ..
        } => {
            if (0.5 * (ta + tb - a0 - b0)).abs() <= CONDUCTION_DRIFT {
                return;
            }
            conductivity.at(0.5 * (ta + tb)) * area / length
        }
        Link::Convection {
            medium,
            area,
            shape,
            speed,
            pressure,
            ..
        } => {
            let film = (0.5 * (ta + tb - a0 - b0)).abs();
            let (d, d0) = (ta - tb, a0 - b0);
            if film <= FILM_DRIFT
                && (d - d0).abs() <= DIFFERENCE_DRIFT * d0.abs() + DIFFERENCE_FLOOR
            {
                return;
            }
            coefficient(medium, shape, ta, tb, pressure, speed, gravity) * area
        }
        Link::Radiation { .. } => return,
    };
    (hot.g, hot.da, hot.coupling) = (fresh, fresh, fresh);
    hot.drift = relative(fresh, hot.factored.0).max(relative(fresh, hot.factored.1));
    *cold = Cold(ta, tb);
}

impl Hot {
    /// The flow (and for radiation its derivatives and their drift) at `ta`, `tb`;
    /// `radiation` is σ·F for a radiation link (zero for the others), `fixed` whether its
    /// second node is held.
    pub fn linearise(&mut self, ta: f64, tb: f64, radiation: f64, fixed: bool) {
        if radiation == 0.0 {
            self.flow = self.g * (ta - tb);
        } else {
            let (a2, b2) = (ta * ta, tb * tb);
            let secant = radiation * (a2 + b2) * (ta + tb);
            self.flow = radiation * (a2 * a2 - b2 * b2);
            self.da = if fixed {
                4.0 * radiation * a2 * ta
            } else {
                secant
            };
            self.coupling = secant;
            self.drift = relative(self.da, self.factored.0).max(relative(secant, self.factored.1));
        }
    }
}

/// A link's two ends, packed: its first node (local), and its second, local if a mass of the
/// island, else the fixed node's id with [`FIXED`] set.
#[derive(Clone, Copy, Debug)]
pub struct Ends(pub u32, pub u32);

/// Marks a fixed node in [`Ends`]; in an incidence list, that the node is the link's first.
pub const FIXED: u32 = 1 << 31;
/// In an incidence list: that the link's second node is fixed.
pub const TO_FIXED: u32 = 1 << 30;

impl Ends {
    /// Its first node (local).
    pub fn a(self) -> usize {
        self.0 as usize
    }

    /// Its second: `Ok(local mass)` or `Err(fixed node)`.
    pub fn b(self) -> Result<usize, usize> {
        if self.1 & FIXED == 0 {
            Ok(self.1 as usize)
        } else {
            Err((self.1 & !FIXED) as usize)
        }
    }
}

/// σ·F of a link if it radiates.
pub fn radiation(link: &Link) -> f64 {
    match *link {
        Link::Radiation { factor, .. } => STEFAN_BOLTZMANN * factor,
        _ => 0.0,
    }
}
