//! Ageing (G1.3c, ADR-0026, `docs/art-style.md` §6b): how long each object has weathered, and
//! how worn it is from use, for the renderer to draw.
//!
//! An object's exposure is years of weathering at the reference rate (outdoors in temperate
//! air, ISO 9223's category C3); each palette token ages by it at its own rate (`weathers_in`).
//! It grows at a rate its surroundings set (air 1, the sea 3, vacuum 0) times the world's pace,
//! kept since the tick it last changed, so nothing is stepped per tick: the exposure at any
//! tick follows. Impacts beyond what a coating resists add wear.

use creeer_math::codec::{DecodeError, Reader, Writer};

/// The world's default pace: years of weathering per second of game time. One year per game
/// day of 6 hours (the home planet's): real rates would be invisible within a play session.
pub const PACE: f64 = 1.0 / 21_600.0;
/// Rates by surroundings: air with oxygen and water (ISO 9223 C3, the reference).
pub const AIR: f64 = 1.0;
/// Under the sea: marine air and water corrode at C5 and beyond, 80–200 µm a year on steel
/// against C3's 25–50 (ISO 9223): three times.
pub const SEA: f64 = 3.0;
/// In vacuum: no oxygen or water, so no rust, patina or rot.
pub const VACUUM: f64 = 0.0;
/// The impact energy a coating takes without chipping (J): of the order ISO 6272's
/// falling-weight test measures for industrial coatings.
pub const RESIST: f64 = 10.0;
/// The impact energy beyond [`RESIST`] that wears an object fully (J): calibrated so that
/// about 25 one-metre drops batter the 200 kg crate (no published rate of visible chipping
/// per joule was found).
pub const FULL_WEAR: f64 = 50_000.0;
/// A change of velocity within one step that counts as an impact (m/s): as a person's jolt
/// (`character::ragdoll`); sliding and gravity change it far less a step.
pub const JOLT: f64 = 1.0;

/// One object's age.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Weathering {
    /// Years weathered at the reference rate, as of tick `since`.
    pub exposure: f64,
    /// Years of weathering per second of game time, since `since`.
    pub rate: f64,
    pub since: u64,
    /// Wear from use, 0 (none) to 1 (battered).
    pub wear: f64,
}

impl Weathering {
    /// A new object at `tick`, weathering at `rate`.
    pub fn new(tick: u64, rate: f64) -> Self {
        Self {
            exposure: 0.0,
            rate,
            since: tick,
            wear: 0.0,
        }
    }

    /// Its exposure at `tick` (years), ticks `dt` seconds long.
    pub fn exposure_at(&self, tick: u64, dt: f64) -> f64 {
        #[allow(clippy::cast_precision_loss)] // ticks below 2⁵³
        let seconds = tick.saturating_sub(self.since) as f64 * dt;
        self.exposure + self.rate * seconds
    }

    /// Weathers at `rate` from `tick` on; whether that changed anything.
    pub fn set_rate(&mut self, tick: u64, dt: f64, rate: f64) -> bool {
        // Exactly the same: rates are a few constants times the pace.
        if rate.to_bits() == self.rate.to_bits() {
            return false;
        }
        self.exposure = self.exposure_at(tick, dt);
        self.since = tick;
        self.rate = rate;
        true
    }

    /// An impact dissipating `energy` (J); whether it wore the object.
    pub fn strike(&mut self, energy: f64) -> bool {
        let excess = energy - RESIST;
        if excess <= 0.0 || self.wear >= 1.0 {
            return false;
        }
        self.wear = (self.wear + excess / FULL_WEAR).min(1.0);
        true
    }

    /// Writes it.
    pub fn write(&self, w: &mut Writer) {
        w.f64(self.exposure);
        w.f64(self.rate);
        w.u64(self.since);
        w.f64(self.wear);
    }

    /// Reads one [`Weathering::write`] wrote.
    ///
    /// # Errors
    /// If the bytes end early.
    pub fn read(r: &mut Reader) -> Result<Self, DecodeError> {
        Ok(Self {
            exposure: r.f64()?,
            rate: r.f64()?,
            since: r.u64()?,
            wear: r.f64()?,
        })
    }
}

/// The rate an object weathers at (years per second at `pace`): under the sea when `depth` (m,
/// of its centre; negative above) is positive, in air when the world has an atmosphere, else
/// in vacuum.
pub fn rate(pace: f64, depth: Option<f64>, has_air: bool) -> f64 {
    let surroundings = if depth.is_some_and(|d| d > 0.0) {
        SEA
    } else if has_air {
        AIR
    } else {
        VACUUM
    };
    pace * surroundings
}

/// The energy (J) an impact dissipated: the kinetic energy a body of `mass` lost in one step
/// from velocity `before` to `after`, if its velocity jolted by more than [`JOLT`]; else none.
pub fn impact_energy(mass: f64, before: [f64; 3], after: [f64; 3]) -> f64 {
    let squared = |v: [f64; 3]| v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
    let jolt = squared([
        after[0] - before[0],
        after[1] - before[1],
        after[2] - before[2],
    ]);
    if jolt <= JOLT * JOLT {
        return 0.0;
    }
    (0.5 * mass * (squared(before) - squared(after))).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f64 = 1.0 / 60.0;

    #[test]
    fn exposure_follows_from_the_rate_and_changes_only_on_events() {
        let mut w = Weathering::new(100, rate(PACE, None, true));
        // One game day of 6 hours outdoors in air: one year.
        let day = 100 + 6 * 3600 * 60;
        assert!((w.exposure_at(day, DT) - 1.0).abs() < 1e-9);
        // Into the sea at noon: the afternoon weathers three times as fast.
        let noon = 100 + 3 * 3600 * 60;
        assert!(w.set_rate(noon, DT, rate(PACE, Some(2.0), true)));
        assert!((w.exposure_at(day, DT) - 2.0).abs() < 1e-9);
        assert!(
            !w.set_rate(noon + 5, DT, rate(PACE, Some(1.0), true)),
            "still the sea"
        );
        // In vacuum it stops where it was.
        assert!(w.set_rate(day, DT, rate(PACE, None, false)));
        assert!((w.exposure_at(day + 1_000_000, DT) - 2.0).abs() < 1e-9);
    }

    #[test]
    fn impacts_beyond_what_a_coating_resists_wear_an_object() {
        let mut w = Weathering::new(0, 0.0);
        // A 200 kg crate dropped from 1 m lands at √(2·9.81·1) and stops: 1962 J.
        let landing = (2.0_f64 * 9.81).sqrt();
        let energy = impact_energy(200.0, [0.0, -landing, 0.0], [0.0; 3]);
        assert!((energy - 1962.0).abs() < 1.0, "{energy}");
        assert!(w.strike(energy));
        assert!((w.wear - (energy - RESIST) / FULL_WEAR).abs() < 1e-12);
        // About 25 such drops batter it; it never passes 1.
        for _ in 0..30 {
            w.strike(energy);
        }
        assert!((w.wear - 1.0).abs() < 1e-12);
        // A light tap, and sliding (no jolt), wear nothing.
        assert!(!Weathering::new(0, 0.0).strike(5.0));
        assert!(impact_energy(200.0, [2.0, 0.0, 0.0], [1.88, 0.0, 0.0]).abs() < 1e-12);
        // A bounce dissipates only what the rebound does not take back.
        let bounce = impact_energy(1.0, [0.0, -4.0, 0.0], [0.0, 2.0, 0.0]);
        assert!((bounce - 0.5 * (16.0 - 4.0)).abs() < 1e-12);
    }

    #[test]
    fn weathering_round_trips_through_bytes() {
        let w = Weathering {
            exposure: 1.25,
            rate: PACE * SEA,
            since: 12_345,
            wear: 0.375,
        };
        let mut writer = Writer::new();
        w.write(&mut writer);
        let bytes = writer.finish();
        let mut r = Reader::new(&bytes);
        assert_eq!(Weathering::read(&mut r).expect("reads"), w);
        r.end().expect("nothing left");
    }
}
