//! Injury from mechanics (G1.2d): what an impact does to a person is set by the deceleration
//! it gives the part struck, not by the height fallen.
//!
//! A step's change of a part's momentum beyond gravity's (`J`) is an impact. Its pulse lasts
//! as long as the part's own compliance against the ground lets it: half a period of the part
//! on that spring, τ = π·√(m/k), with a person's stiffness (the head behind a helmet's liner,
//! the torso behind ribs and suit). The pulse's mean deceleration is J/(m·τ), a half-sine's
//! peak π/2 of it.
//! - **The head:** the head-injury criterion of a half-sine, HIC = τ·(ā/g)^2.5 (its window
//!   the pulse, under 15 ms); the chance of a serious injury (AIS 3+) from it by Prasad and
//!   Mertz's curve (1985).
//! - **The torso and the rest:** the peak deceleration against the chest's tolerance
//!   (serious from about 60 g over a few milliseconds).
//!
//! The ground's own compliance is left out: stiff ground (rock, concrete) is far stiffer than
//! a person, and soft soil's yielding is not modelled (`docs/future-improvements.md`). Water
//! gives no contact: its drag and buoyancy slow a body over a long way.

use creeer_math::det;

/// Standard gravity (m/s²).
const G: f64 = 9.806_65;
/// A head's stiffness against a rigid surface behind a helmet's liner (N/m).
pub const HEAD_STIFFNESS: f64 = 4.0e5;
/// A body's (N/m) striking the ground through hips, back and shoulders: its skeleton's
/// paths, about 10⁶ (a chest's against a blow, 1–3·10⁵, gave falls from 20 m no injury).
pub const TORSO_STIFFNESS: f64 = 1.0e6;
/// The peak deceleration a torso takes without injury (g), and where its injury is certain.
const TORSO_TOLERANCE: (f64, f64) = (30.0, 90.0);

/// An impact on a part: its peak deceleration (g) and its duration (s).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pulse {
    pub peak: f64,
    pub duration: f64,
}

/// The pulse of an impact `impulse` (N·s, beyond the part's weight) on a part of `mass` (kg)
/// and `stiffness` (N/m), if it is one.
pub fn pulse(impulse: f64, mass: f64, stiffness: f64) -> Option<Pulse> {
    if impulse <= 0.0 || mass <= 0.0 {
        return None;
    }
    let duration = std::f64::consts::PI * (mass / stiffness).sqrt();
    let mean = impulse / (mass * duration) / G;
    Some(Pulse {
        peak: std::f64::consts::FRAC_PI_2 * mean,
        duration,
    })
}

/// A half-sine pulse's head-injury criterion (its window the pulse itself, at most 15 ms).
pub fn hic(p: Pulse) -> f64 {
    let window = p.duration.min(0.015);
    let mean = p.peak / std::f64::consts::FRAC_PI_2;
    window * det::pow(mean, 2.5)
}

/// The chance of a serious head injury (AIS 3+) at `hic` (Prasad & Mertz 1985, in the form
/// NHTSA uses).
pub fn serious_head(hic: f64) -> f64 {
    if hic <= 0.0 {
        return 0.0;
    }
    1.0 / (1.0 + det::exp(3.39 + 200.0 / hic - 0.003_72 * hic))
}

/// The share of a torso's health a pulse takes: none below its tolerance, all at its limit.
pub fn torso(p: Pulse) -> f64 {
    let (from, to) = TORSO_TOLERANCE;
    ((p.peak - from) / (to - from)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_half_sine_pulse_has_its_closed_form() {
        // 80 kg stopped from 5 m/s on 2·10⁵ N/m: τ = π·√(80/2e5) = 62.8 ms, mean 8.1 g, peak 12.7 g.
        let p = pulse(80.0 * 5.0, 80.0, 2.0e5).unwrap();
        assert!((p.duration - 0.06283).abs() < 1e-4, "{}", p.duration);
        assert!((p.peak - 12.74).abs() < 0.05, "{}", p.peak);
        assert_eq!(torso(p), 0.0);
        assert!(pulse(0.0, 80.0, TORSO_STIFFNESS).is_none());
    }

    #[test]
    fn the_head_injury_curve_rises_through_its_known_points() {
        // Prasad–Mertz (NHTSA): about 13 % at HIC 500, half at 1000, nine in ten at 1500.
        let at = |h: f64| serious_head(h);
        assert!(at(100.0) < 0.01);
        assert!((0.08..0.20).contains(&at(500.0)), "{}", at(500.0));
        assert!((0.40..0.65).contains(&at(1000.0)), "{}", at(1000.0));
        assert!(at(1500.0) > 0.8, "{}", at(1500.0));
    }
}

/// The change of body `part`'s velocity over `world`'s last step (from `before`) that neither
/// gravity nor its own loads explain: what it met gave it (m/s).
pub fn jolt(
    world: &creeer_physics::world::World,
    part: usize,
    before: creeer_math::glam::DVec3,
) -> creeer_math::glam::DVec3 {
    let s = &world.bodies[part].structure;
    let g = (s.planet.as_ref()).map_or(s.gravity, |p| p.acceleration(s.position[0]));
    // What the body's own loads explain is no impact (its legs' braking: merged with the
    // body's strike into one long one, it diluted a 10 m fall to bruises).
    let own = s.load[0] / s.masses()[0];
    s.velocity[0] - before - (g + own) * world.dt()
}

/// A step's change of a part's velocity beyond this (m/s, besides gravity's) belongs to an
/// impact: 3 g over a step, more than a landing on the legs keeps up.
const STRIKING: f64 = 0.5;

/// The impacts on a person's parts as they happen, each summed over the steps it lasts (an
/// impact spreads over two or three steps at 60 Hz, and taken step by step its pulses were a
/// third as hard: a fall from 20 m was not fatal).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Strikes {
    /// Each part's impulse so far in the impact it is in (N·s; nought when in none), and the
    /// steps it has lasted.
    going: Vec<(f64, u32)>,
}

impl Strikes {
    /// After a step: part `k` (body `part` of `world`, moving at `before` when the step began)
    /// of `stiffness` (the head's judged by HIC). Returns the harm of an impact that ended
    /// with this step, as a share of a person's health.
    pub fn after_step(
        &mut self,
        world: &creeer_physics::world::World,
        (k, part): (usize, usize),
        before: creeer_math::glam::DVec3,
        (stiffness, head): (f64, bool),
    ) -> f64 {
        if self.going.len() <= k {
            self.going.resize(k + 1, (0.0, 0));
        }
        let mass = world.bodies[part].structure.masses()[0];
        let change = jolt(world, part, before).length();
        if change > STRIKING {
            self.going[k].0 += change * mass;
            self.going[k].1 += 1;
            return 0.0;
        }
        self.end(world, (k, part), (stiffness, head))
    }

    /// Part `k`'s impact (if it is in one) ends now: its harm, as [`Strikes::after_step`]'s
    /// (a character's body when it falls: its segments take the rest of the impact).
    pub fn end(
        &mut self,
        world: &creeer_physics::world::World,
        (k, part): (usize, usize),
        (stiffness, head): (f64, bool),
    ) -> f64 {
        let Some(going) = self.going.get_mut(k) else {
            return 0.0;
        };
        let mass = world.bodies[part].structure.masses()[0];
        let (impulse, steps) = std::mem::take(going);
        let Some(mut p) = pulse(impulse, mass, stiffness) else {
            return 0.0;
        };
        // At least as long as it was seen to last (the legs stopping a landing over 150 ms: not
        // the body's own 32 ms pulse).
        let seen = f64::from(steps) * world.dt();
        if seen > p.duration {
            p.peak *= p.duration / seen;
            p.duration = seen;
        }
        if head { serious_head(hic(p)) } else { torso(p) }
    }
}
