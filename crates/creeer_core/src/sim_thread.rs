//! Runs a simulation on its own thread at a fixed tick rate (plan.md §1, principle 9).
//!
//! The simulation publishes its two most recent states after every tick. The render thread
//! picks up the latest pair whenever it draws a frame and interpolates between them, so:
//! - the frame rate never depends on how long a tick takes, and
//! - motion stays smooth even though the simulation runs at 60 Hz and the monitor at 144 Hz.
//!
//! The rendered image is therefore up to one tick (16.7 ms) behind the simulation, the
//! standard trade-off for fixed-timestep games.

use crate::clock::FixedClock;
use crate::stats::TimingStats;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Something that advances in fixed steps and can describe its state for rendering.
pub trait Simulation: Send + 'static {
    /// Everything the renderer needs from one tick. Shared immutably via `Arc`.
    type Snapshot: Send + Sync + 'static;

    /// Advances the simulation by `dt` seconds.
    fn step(&mut self, dt: f64);

    /// Captures the current state.
    fn snapshot(&self) -> Self::Snapshot;
}

/// The two most recent simulation states.
#[derive(Debug)]
pub struct Published<T> {
    /// State one tick before `current`.
    pub previous: Arc<T>,
    /// Newest state.
    pub current: Arc<T>,
    /// Tick number of `current`.
    pub tick: u64,
    /// When `current` was published.
    pub published_at: Instant,
    /// Seconds per tick.
    pub dt: f64,
}

impl<T> Clone for Published<T> {
    fn clone(&self) -> Self {
        Self {
            previous: Arc::clone(&self.previous),
            current: Arc::clone(&self.current),
            tick: self.tick,
            published_at: self.published_at,
            dt: self.dt,
        }
    }
}

impl<T> Published<T> {
    /// Interpolation factor between `previous` (0) and `current` (1) for a frame drawn at
    /// `now`. Clamped to 1: when the simulation falls behind, objects pause rather than
    /// being extrapolated into states that never happen.
    pub fn alpha(&self, now: Instant) -> f64 {
        let since = now
            .saturating_duration_since(self.published_at)
            .as_secs_f64();
        (since / self.dt).clamp(0.0, 1.0)
    }
}

struct Shared<T> {
    latest: Mutex<Published<T>>,
    tick_times: Mutex<TimingStats>,
    stop: AtomicBool,
    /// Artificial extra work per tick in microseconds, for testing render/sim decoupling.
    extra_tick_delay_us: AtomicU64,
}

/// Handle to a simulation running on its own thread. Dropping it stops the thread.
pub struct SimThread<S: Simulation> {
    shared: Arc<Shared<S::Snapshot>>,
    handle: Option<JoinHandle<()>>,
}

impl<S: Simulation> std::fmt::Debug for SimThread<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SimThread")
            .field("running", &self.handle.is_some())
            .finish_non_exhaustive()
    }
}

/// Longest backlog of ticks run in one go before real time is dropped.
const MAX_CATCH_UP_TICKS: u32 = 5;

impl<S: Simulation> SimThread<S> {
    /// Starts `sim` on a new thread, ticking at `tick_rate_hz`.
    ///
    /// # Panics
    /// If the OS cannot spawn a thread.
    pub fn spawn(mut sim: S, tick_rate_hz: u32) -> Self {
        let mut clock = FixedClock::new(tick_rate_hz, MAX_CATCH_UP_TICKS);
        let initial = Arc::new(sim.snapshot());
        let shared = Arc::new(Shared {
            latest: Mutex::new(Published {
                previous: Arc::clone(&initial),
                current: initial,
                tick: 0,
                published_at: Instant::now(),
                dt: clock.dt(),
            }),
            tick_times: Mutex::new(TimingStats::new(300)),
            stop: AtomicBool::new(false),
            extra_tick_delay_us: AtomicU64::new(0),
        });

        let thread_shared = Arc::clone(&shared);
        let handle = thread::Builder::new()
            .name("simulation".into())
            .spawn(move || run(&mut sim, &mut clock, &thread_shared))
            .expect("failed to spawn simulation thread");

        Self {
            shared,
            handle: Some(handle),
        }
    }

    /// The latest published pair of states (cheap: clones two `Arc`s).
    pub fn latest(&self) -> Published<S::Snapshot> {
        lock(&self.shared.latest).clone()
    }

    /// Recent tick durations (time spent inside `step`).
    pub fn tick_times(&self) -> TimingStats {
        lock(&self.shared.tick_times).clone()
    }

    /// Adds artificial work to every tick (debug tool to prove render/sim decoupling).
    pub fn set_extra_tick_delay(&self, delay: Duration) {
        let micros = u64::try_from(delay.as_micros()).unwrap_or(u64::MAX);
        self.shared
            .extra_tick_delay_us
            .store(micros, Ordering::Relaxed);
    }

    /// Stops the thread and waits for it to finish.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            // A panicking simulation already printed its message; nothing more to do here.
            let _ = handle.join();
        }
    }
}

impl<S: Simulation> Drop for SimThread<S> {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn run<S: Simulation>(sim: &mut S, clock: &mut FixedClock, shared: &Shared<S::Snapshot>) {
    let mut last = Instant::now();
    let mut current = Arc::clone(&lock(&shared.latest).current);
    // The profile's clock, held while anyone wants a profile and nobody else holds it.
    let mut frames: Option<crate::profile::Clock> = None;
    while !shared.stop.load(Ordering::Relaxed) {
        let now = Instant::now();
        let ticks = clock.advance(now - last);
        last = now;

        let mut previous = None;
        for _ in 0..ticks {
            let start = Instant::now();
            let scope = crate::profile::scope("simulation tick");
            sim.step(clock.dt());
            drop(scope);
            let extra = shared.extra_tick_delay_us.load(Ordering::Relaxed);
            if extra > 0 {
                thread::sleep(Duration::from_micros(extra));
            }
            lock(&shared.tick_times).push(start.elapsed());
            previous = Some(std::mem::replace(&mut current, Arc::new(sim.snapshot())));
            if crate::profile::enabled() {
                frames = frames
                    .filter(crate::profile::Clock::holds)
                    .or_else(|| crate::profile::clock(1));
                if let Some(f) = &mut frames {
                    f.end_tick(clock.tick());
                }
            } else {
                frames = None;
            }
        }
        if let Some(previous) = previous {
            *lock(&shared.latest) = Published {
                previous,
                current: Arc::clone(&current),
                tick: clock.tick(),
                published_at: Instant::now(),
                dt: clock.dt(),
            };
        }

        // Sleep until the next tick is due.
        let remaining = (1.0 - clock.alpha()) * clock.dt();
        thread::sleep(Duration::from_secs_f64(remaining.max(0.0)));
    }
}

/// Locks a mutex, recovering the data if another thread panicked while holding it.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Counts its own ticks and accumulated simulated time.
    struct Counter {
        ticks: u64,
        time: f64,
    }

    impl Simulation for Counter {
        type Snapshot = (u64, f64);
        fn step(&mut self, dt: f64) {
            self.ticks += 1;
            self.time += dt;
        }
        fn snapshot(&self) -> (u64, f64) {
            (self.ticks, self.time)
        }
    }

    fn wait_for_tick(sim: &SimThread<Counter>, tick: u64) -> Published<(u64, f64)> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let latest = sim.latest();
            if latest.tick >= tick {
                return latest;
            }
            assert!(
                Instant::now() < deadline,
                "simulation did not reach tick {tick}"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn publishes_consecutive_states() {
        let sim = SimThread::spawn(
            Counter {
                ticks: 0,
                time: 0.0,
            },
            120,
        );
        let latest = wait_for_tick(&sim, 5);
        let (prev, curr) = (*latest.previous, *latest.current);
        assert_eq!(
            curr.0, latest.tick,
            "snapshot matches published tick number"
        );
        assert_eq!(curr.0 - prev.0, 1, "previous is exactly one tick older");
        assert!((curr.1 - prev.1 - 1.0 / 120.0).abs() < 1e-12);
        sim.stop();
    }

    #[test]
    fn tick_rate_matches_real_time_roughly() {
        let sim = SimThread::spawn(
            Counter {
                ticks: 0,
                time: 0.0,
            },
            100,
        );
        let start = Instant::now();
        thread::sleep(Duration::from_millis(300));
        let ticks = sim.latest().tick;
        #[allow(clippy::cast_precision_loss)]
        let rate = ticks as f64 / start.elapsed().as_secs_f64();
        // Generous bounds: CI machines are noisy, this only catches gross errors.
        assert!((50.0..=110.0).contains(&rate), "measured {rate} ticks/s");
        sim.stop();
    }

    #[test]
    fn alpha_is_clamped_to_one_when_the_simulation_lags() {
        let published = Published {
            previous: Arc::new(()),
            current: Arc::new(()),
            tick: 1,
            published_at: Instant::now(),
            dt: 0.01,
        };
        let later = published.published_at + Duration::from_millis(5);
        assert!((published.alpha(later) - 0.5).abs() < 1e-9);
        let much_later = published.published_at + Duration::from_secs(1);
        assert!((published.alpha(much_later) - 1.0).abs() < f64::EPSILON);
    }

    /// The F0 decoupling requirement, automated: while every tick takes 40 ms (far over the
    /// 16.7 ms budget), the render thread must still get the latest state instantly.
    #[test]
    fn render_thread_is_never_blocked_by_a_slow_tick() {
        let sim = SimThread::spawn(
            Counter {
                ticks: 0,
                time: 0.0,
            },
            60,
        );
        sim.set_extra_tick_delay(Duration::from_millis(40));
        thread::sleep(Duration::from_millis(100)); // let slow ticks start
        let mut worst = Duration::ZERO;
        let end = Instant::now() + Duration::from_millis(400);
        while Instant::now() < end {
            let start = Instant::now();
            let _ = sim.latest();
            worst = worst.max(start.elapsed());
            thread::sleep(Duration::from_millis(1)); // like a 1000 FPS render loop
        }
        assert!(
            worst < Duration::from_millis(5),
            "latest() blocked for {worst:?}"
        );
        // The simulation itself slowed down instead of skipping physics.
        let ticks = sim.latest().tick;
        assert!(
            ticks < 20,
            "{ticks} ticks in 0.5 s means the delay was ignored"
        );
        sim.stop();
    }

    #[test]
    fn dropping_the_handle_stops_the_thread() {
        let sim = SimThread::spawn(
            Counter {
                ticks: 0,
                time: 0.0,
            },
            60,
        );
        drop(sim); // Must return promptly instead of hanging.
    }
}
