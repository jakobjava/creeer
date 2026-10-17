//! Energy between domains that step at different rates, and when each steps.

use crate::ledger::Sum;

/// Energy passed from one domain to another: the giver adds what it gives at each of its
/// steps, the taker takes all that came since its last step. Nothing is lost or made between
/// rates (a wire heating at 60 Hz feeds a thermal network stepping at 20 Hz exactly the heat
/// it made).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Exchange {
    pending: Sum,
}

impl Exchange {
    /// The giver passes `joules`.
    pub fn give(&mut self, joules: f64) {
        self.pending.add(joules);
    }

    /// The taker takes all that came since it last took.
    pub fn take(&mut self) -> f64 {
        let all = self.pending.value();
        self.pending = Sum::default();
        all
    }

    /// What has come and not been taken.
    pub fn pending(&self) -> f64 {
        self.pending.value()
    }
}

/// When a domain steps: every `every` ticks, on the ticks where `tick % every == phase`
/// (phases spread the slow domains over the ticks between their steps).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Schedule {
    /// Ticks between its steps.
    pub every: u64,
    /// Which tick of those it steps on.
    pub phase: u64,
}

impl Schedule {
    /// Every tick.
    pub const EVERY_TICK: Self = Self { every: 1, phase: 0 };

    /// Whether it steps on tick `tick`.
    pub fn due(&self, tick: u64) -> bool {
        tick % self.every == self.phase % self.every
    }

    /// Its step (s), the tick being `tick_seconds` long.
    pub fn step(&self, tick_seconds: f64) -> f64 {
        #[allow(clippy::cast_precision_loss)] // a few ticks
        let every = self.every as f64;
        tick_seconds * every
    }
}
