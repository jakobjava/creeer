//! Rolling timing statistics (frame times, tick times) with percentiles.
//!
//! Averages hide stutter: 59 frames of 10 ms plus one of 200 ms still average 13 ms. The
//! perf gates therefore look at p50 / p99 / max (plan.md §5).

use std::collections::VecDeque;
use std::time::Duration;

/// The most recent `capacity` durations, in milliseconds.
#[derive(Clone, Debug)]
pub struct TimingStats {
    samples_ms: VecDeque<f64>,
    capacity: usize,
}

impl TimingStats {
    /// # Panics
    /// If `capacity` is zero.
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "capacity must be positive");
        Self {
            samples_ms: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Records one sample, evicting the oldest when full.
    pub fn push(&mut self, duration: Duration) {
        if self.samples_ms.len() == self.capacity {
            self.samples_ms.pop_front();
        }
        self.samples_ms.push_back(duration.as_secs_f64() * 1000.0);
    }

    /// Number of samples held.
    pub fn len(&self) -> usize {
        self.samples_ms.len()
    }

    /// True when no samples have been recorded.
    pub fn is_empty(&self) -> bool {
        self.samples_ms.is_empty()
    }

    /// Samples in milliseconds, oldest first.
    pub fn samples_ms(&self) -> impl ExactSizeIterator<Item = f64> + '_ {
        self.samples_ms.iter().copied()
    }

    /// Arithmetic mean in milliseconds, or `None` when empty.
    pub fn mean_ms(&self) -> Option<f64> {
        if self.is_empty() {
            return None;
        }
        #[allow(clippy::cast_precision_loss)] // sample counts are small
        let n = self.len() as f64;
        Some(self.samples_ms.iter().sum::<f64>() / n)
    }

    /// Nearest-rank percentile (`p` in 0..=100) in milliseconds, or `None` when empty.
    pub fn percentile_ms(&self, p: f64) -> Option<f64> {
        if self.is_empty() {
            return None;
        }
        let mut sorted: Vec<f64> = self.samples_ms.iter().copied().collect();
        sorted.sort_by(f64::total_cmp);
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )]
        let rank = ((p.clamp(0.0, 100.0) / 100.0) * sorted.len() as f64).ceil() as usize;
        Some(sorted[rank.clamp(1, sorted.len()) - 1])
    }

    /// Largest sample in milliseconds, or `None` when empty.
    pub fn max_ms(&self) -> Option<f64> {
        self.samples_ms.iter().copied().max_by(f64::total_cmp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats_from_ms(values: &[u64]) -> TimingStats {
        let mut stats = TimingStats::new(values.len().max(1));
        for &v in values {
            stats.push(Duration::from_millis(v));
        }
        stats
    }

    #[test]
    fn empty_stats_have_no_values() {
        let stats = TimingStats::new(4);
        assert!(stats.mean_ms().is_none());
        assert!(stats.percentile_ms(50.0).is_none());
        assert!(stats.max_ms().is_none());
    }

    #[test]
    fn percentiles_use_nearest_rank() {
        let stats = stats_from_ms(&[10, 20, 30, 40, 50, 60, 70, 80, 90, 100]);
        assert_eq!(stats.percentile_ms(50.0), Some(50.0));
        assert_eq!(stats.percentile_ms(99.0), Some(100.0));
        assert_eq!(stats.percentile_ms(0.0), Some(10.0));
        assert_eq!(stats.percentile_ms(100.0), Some(100.0));
    }

    #[test]
    fn a_single_hitch_shows_in_p99_but_barely_in_the_mean() {
        let mut values = vec![10; 99];
        values.push(200);
        let stats = stats_from_ms(&values);
        let mean = stats.mean_ms().unwrap();
        assert!((mean - 11.9).abs() < 1e-9);
        assert_eq!(stats.percentile_ms(50.0), Some(10.0));
        assert_eq!(stats.max_ms(), Some(200.0));
    }

    #[test]
    fn oldest_samples_are_evicted() {
        let mut stats = TimingStats::new(3);
        for ms in [1, 2, 3, 4, 5] {
            stats.push(Duration::from_millis(ms));
        }
        let kept: Vec<f64> = stats.samples_ms().collect();
        assert_eq!(kept, vec![3.0, 4.0, 5.0]);
    }
}

/// Measures how fast a quantity grows per real second, over a sliding time window.
/// Used for the simulation speed readout: simulated seconds per real second.
#[derive(Clone, Debug)]
pub struct RateMeter {
    window: Duration,
    samples: VecDeque<(std::time::Instant, f64)>,
}

impl RateMeter {
    /// A meter averaging over the last `window` of real time.
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            samples: VecDeque::new(),
        }
    }

    /// Records `value` as observed at `now`. Instants must not go backwards.
    pub fn record(&mut self, now: std::time::Instant, value: f64) {
        self.samples.push_back((now, value));
        while let Some(&(oldest, _)) = self.samples.front() {
            if now.duration_since(oldest) > self.window && self.samples.len() > 2 {
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }

    /// Change of the value per real second, or `None` until two samples are apart in time.
    pub fn rate(&self) -> Option<f64> {
        let (&(t0, v0), &(t1, v1)) = (self.samples.front()?, self.samples.back()?);
        let elapsed = t1.duration_since(t0).as_secs_f64();
        (elapsed > 0.0).then(|| (v1 - v0) / elapsed)
    }
}

#[cfg(test)]
mod rate_tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn measures_rate_over_the_window() {
        let start = Instant::now();
        let mut meter = RateMeter::new(Duration::from_secs(1));
        assert!(meter.rate().is_none());
        // Simulated time advancing at 0.36x real time, sampled every 10 ms for 3 s.
        for i in 0..=300u32 {
            let t = start + Duration::from_millis(u64::from(i) * 10);
            meter.record(t, 0.36 * f64::from(i) * 0.01);
        }
        let rate = meter.rate().unwrap();
        assert!((rate - 0.36).abs() < 1e-9, "rate {rate}");
        // Old samples are discarded: only about one second's worth is kept.
        assert!(meter.samples.len() <= 102);
    }
}
