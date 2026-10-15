//! Fixed-timestep simulation clock.
//!
//! The simulation always advances in identical steps (e.g. 1/60 s), no matter how irregular
//! real frame times are. That keeps physics stable and reproducible. Real time is fed in, and
//! the clock answers "how many ticks are due now?" plus an interpolation factor the renderer
//! uses to blend between the last two simulation states.
//!
//! Time is accumulated in exact integer units (nanoseconds × tick rate), so there is no
//! floating-point drift: after exactly one real second at 60 Hz, exactly 60 ticks have run.

use std::time::Duration;

const NANOS_PER_SECOND: u128 = 1_000_000_000;

/// Fixed-timestep accumulator. See the module docs.
#[derive(Clone, Debug)]
pub struct FixedClock {
    tick_rate_hz: u32,
    /// Accumulated real time not yet consumed by ticks, in units of 1 / (1e9 × rate) s.
    /// One tick consumes exactly `NANOS_PER_SECOND` units.
    accumulator: u128,
    tick: u64,
    max_ticks_per_advance: u32,
    dropped: Duration,
}

impl FixedClock {
    /// Creates a clock ticking at `tick_rate_hz`. When the simulation falls behind, at most
    /// `max_ticks_per_advance` ticks are run per call; older backlog is dropped (the game slows
    /// down instead of freezing in a "spiral of death").
    ///
    /// # Panics
    /// If `tick_rate_hz` or `max_ticks_per_advance` is zero.
    pub fn new(tick_rate_hz: u32, max_ticks_per_advance: u32) -> Self {
        assert!(tick_rate_hz > 0, "tick rate must be positive");
        assert!(
            max_ticks_per_advance > 0,
            "must allow at least one tick per advance"
        );
        Self {
            tick_rate_hz,
            accumulator: 0,
            tick: 0,
            max_ticks_per_advance,
            dropped: Duration::ZERO,
        }
    }

    /// Feeds `elapsed` real time and returns how many ticks should be simulated now.
    /// The tick counter advances by the returned amount.
    pub fn advance(&mut self, elapsed: Duration) -> u32 {
        self.accumulator += elapsed.as_nanos() * u128::from(self.tick_rate_hz);
        let due = self.accumulator / NANOS_PER_SECOND;
        let max = u128::from(self.max_ticks_per_advance);
        let ticks = if due > max {
            let dropped_units = (due - max) * NANOS_PER_SECOND;
            self.dropped += units_to_duration(dropped_units, self.tick_rate_hz);
            self.accumulator -= dropped_units;
            max
        } else {
            due
        };
        self.accumulator -= ticks * NANOS_PER_SECOND;
        let ticks = u32::try_from(ticks).expect("bounded by max_ticks_per_advance");
        self.tick += u64::from(ticks);
        ticks
    }

    /// Fraction of the next tick that has already elapsed, in `[0, 1)`. The renderer draws
    /// `lerp(previous_state, current_state, alpha)`.
    pub fn alpha(&self) -> f64 {
        // Exact integer ratio converted once; both values are < 2^53 in practice.
        #[allow(clippy::cast_precision_loss)]
        let alpha = self.accumulator as f64 / NANOS_PER_SECOND as f64;
        alpha
    }

    /// Number of ticks simulated so far.
    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Simulated seconds per tick.
    pub fn dt(&self) -> f64 {
        1.0 / f64::from(self.tick_rate_hz)
    }

    /// Ticks per second.
    pub fn tick_rate_hz(&self) -> u32 {
        self.tick_rate_hz
    }

    /// Total real time dropped because the simulation could not keep up.
    pub fn dropped(&self) -> Duration {
        self.dropped
    }
}

fn units_to_duration(units: u128, tick_rate_hz: u32) -> Duration {
    let nanos = units / u128::from(tick_rate_hz);
    Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn one_real_second_is_exactly_sixty_ticks() {
        let mut clock = FixedClock::new(60, 1000);
        // 1/60 s is not a whole number of nanoseconds, so float accumulators drift; ours must not.
        let mut total = 0;
        for _ in 0..1000 {
            total += clock.advance(Duration::from_millis(1));
        }
        assert_eq!(total, 60);
        assert_eq!(clock.tick(), 60);
        assert!(clock.alpha() < 1e-12);
    }

    #[test]
    fn alpha_reports_partial_tick() {
        let mut clock = FixedClock::new(50, 10); // 20 ms ticks
        assert_eq!(clock.advance(Duration::from_millis(25)), 1);
        assert!((clock.alpha() - 0.25).abs() < 1e-12);
    }

    #[test]
    fn backlog_beyond_the_cap_is_dropped_not_queued() {
        let mut clock = FixedClock::new(60, 4);
        // A 1 s hitch (e.g. window dragged) must not trigger 60 catch-up ticks.
        assert_eq!(clock.advance(Duration::from_secs(1)), 4);
        let dropped = clock.dropped().as_secs_f64();
        assert!((dropped - 56.0 / 60.0).abs() < 1e-6, "dropped {dropped}");
        // Afterwards the clock behaves normally again.
        assert_eq!(clock.advance(Duration::from_nanos(16_666_667)), 1);
    }

    proptest! {
        /// However real time is chopped into frames, the total tick count only depends on the
        /// total time.
        #[test]
        fn tick_count_is_independent_of_frame_pacing(
            rate in 1u32..1000,
            frames in proptest::collection::vec(0u64..50_000_000, 0..200),
        ) {
            let mut clock = FixedClock::new(rate, u32::MAX);
            let total_ns: u128 = frames.iter().map(|&n| u128::from(n)).sum();
            let mut ticks = 0u64;
            for nanos in frames {
                ticks += u64::from(clock.advance(Duration::from_nanos(nanos)));
            }
            let expected = total_ns * u128::from(rate) / NANOS_PER_SECOND;
            prop_assert_eq!(u128::from(ticks), expected);
            prop_assert!((0.0..1.0).contains(&clock.alpha()));
        }
    }
}
