//! Islands that stop changing stop being solved.

/// Steps an island must hardly change for before it sleeps.
pub const STILL_STEPS: u32 = 30;

/// An island's sleep: it sleeps after [`STILL_STEPS`] steps in which nothing changed by more
/// than its threshold, and wakes as soon as anything acting on it changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sleep {
    still: u32,
    asleep: bool,
}

impl Sleep {
    /// After a step in which its largest change was `change`: whether it may now sleep
    /// (changes up to `threshold` count as none).
    pub fn after_step(&mut self, change: f64, threshold: f64) -> bool {
        if change <= threshold {
            self.still += 1;
        } else {
            self.still = 0;
        }
        self.asleep = self.still >= STILL_STEPS;
        self.asleep
    }

    /// Something acting on it changed.
    pub fn wake(&mut self) {
        *self = Self::default();
    }

    /// Whether it sleeps.
    pub fn asleep(&self) -> bool {
        self.asleep
    }
}
