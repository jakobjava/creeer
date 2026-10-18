//! A profiler of our own (F7.6, ADR-0021): named scopes timed on every thread, collected
//! into one frame per tick, the last ten seconds of them kept.
//!
//! - **Nothing when off.** A scope costs one relaxed atomic load while nobody wants a
//!   profile ([`want`]); then it records its name, start, end and depth into its thread's
//!   buffer when it ends.
//! - **One clock.** A process's ticks are cut into frames by whoever holds its [`Clock`]
//!   (a server's loop before a client's simulation thread). Spans of every thread, the
//!   pool's workers included, go into the frame in which they end.
//! - Frames carry their own names, so a server's can be sent to a client and shown as its
//!   own (`creeer_net`'s `ServerMessage::Profile`).

use std::cell::Cell;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::Instant;

/// Frames kept: ten seconds at 60 ticks a second.
pub const KEPT: usize = 600;

/// How many want a profile; scopes record while it is above nought.
static WANTED: AtomicUsize = AtomicUsize::new(0);
/// Who holds the clock (`Clock::token`; nought: nobody), and the claims made so far.
static OWNER: AtomicU64 = AtomicU64::new(0);
static NEXT_CLAIM: AtomicU64 = AtomicU64::new(1);

/// One span of one frame: what ran (an index into the frame's names), on which thread (an
/// index into its threads), how deep in other spans, and when (ns from the frame's start;
/// a span begun before it starts at nought).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub name: u16,
    pub thread: u16,
    pub depth: u8,
    pub start: u64,
    pub end: u64,
}

/// One tick's spans.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Frame {
    /// The tick it ends.
    pub tick: u64,
    /// Its length (ns): from the frame before's end to its own.
    pub duration: u64,
    pub names: Vec<String>,
    pub threads: Vec<String>,
    pub spans: Vec<Span>,
}

impl Frame {
    /// Total time (ns) of each name's outermost spans on each thread, most first: where
    /// the frame went (a span inside one of the same name is not counted twice).
    pub fn totals(&self) -> Vec<(&str, u64)> {
        let mut sums = vec![0u64; self.names.len()];
        // Per thread in order of start (outer first): the spans still open around each.
        let mut order: Vec<&Span> = self.spans.iter().collect();
        order.sort_by_key(|s| (s.thread, s.start, s.depth));
        let mut open: Vec<&Span> = Vec::new();
        for s in order {
            open.retain(|o| o.thread == s.thread && o.end > s.start && o.depth < s.depth);
            if !open.iter().any(|o| o.name == s.name) {
                sums[usize::from(s.name)] += s.end - s.start;
            }
            open.push(s);
        }
        let mut out: Vec<(&str, u64)> = self.names.iter().map(String::as_str).zip(sums).collect();
        out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        out
    }
}

/// A span as its thread recorded it.
struct Raw {
    name: &'static str,
    depth: u8,
    start: u64,
    end: u64,
}

struct Buffer {
    thread: String,
    spans: Mutex<Vec<Raw>>,
}

struct Shared {
    epoch: Instant,
    buffers: Mutex<Vec<Arc<Buffer>>>,
    /// The frames kept, each with its number in the process (from 1).
    frames: Mutex<VecDeque<(u64, Frame)>>,
}

fn shared() -> &'static Shared {
    static SHARED: OnceLock<Shared> = OnceLock::new();
    SHARED.get_or_init(|| Shared {
        epoch: Instant::now(),
        buffers: Mutex::new(Vec::new()),
        frames: Mutex::new(VecDeque::with_capacity(KEPT)),
    })
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn now() -> u64 {
    u64::try_from(shared().epoch.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

thread_local! {
    static DEPTH: Cell<u8> = const { Cell::new(0) };
    static BUFFER: Arc<Buffer> = {
        let index = lock(&shared().buffers).len();
        let current = std::thread::current();
        let buffer = Arc::new(Buffer {
            thread: current.name().map_or_else(|| format!("thread {index}"), str::to_owned),
            spans: Mutex::new(Vec::new()),
        });
        lock(&shared().buffers).push(Arc::clone(&buffer));
        buffer
    };
}

/// Whether scopes record now.
pub fn enabled() -> bool {
    WANTED.load(Ordering::Relaxed) > 0
}

/// A wish for a profile: scopes record while one is held.
#[derive(Debug)]
pub struct Want(());

/// Wants a profile until the returned guard is dropped.
pub fn want() -> Want {
    WANTED.fetch_add(1, Ordering::Relaxed);
    Want(())
}

impl Drop for Want {
    fn drop(&mut self) {
        WANTED.fetch_sub(1, Ordering::Relaxed);
    }
}

/// A named scope, timed until dropped (while profiling).
#[must_use = "a scope times until it is dropped"]
#[derive(Debug)]
pub struct Scope {
    name: &'static str,
    /// When it began (ns from the epoch); `u64::MAX` when not profiling.
    start: u64,
}

/// Starts timing `name` until the returned scope is dropped.
#[inline]
pub fn scope(name: &'static str) -> Scope {
    if !enabled() {
        return Scope {
            name,
            start: u64::MAX,
        };
    }
    DEPTH.with(|d| d.set(d.get().saturating_add(1)));
    Scope { name, start: now() }
}

impl Drop for Scope {
    #[inline]
    fn drop(&mut self) {
        if self.start == u64::MAX {
            return;
        }
        let depth = DEPTH.with(|d| {
            let depth = d.get().saturating_sub(1);
            d.set(depth);
            depth
        });
        let raw = Raw {
            name: self.name,
            depth,
            start: self.start,
            end: now(),
        };
        BUFFER.with(|b| lock(&b.spans).push(raw));
    }
}

/// The right to cut a process's ticks into frames: one holder at a time, a higher priority
/// taking it from a lower (a hosted server's loop, 2, from the client's simulation thread,
/// 1, which runs on beside it).
#[derive(Debug)]
pub struct Clock {
    /// Its claim (`OWNER`'s value while it holds the clock).
    token: u64,
    last: u64,
}

/// Claims the clock at `priority`, if nobody holds it at that priority or above.
pub fn clock(priority: u8) -> Option<Clock> {
    let token = NEXT_CLAIM.fetch_add(1, Ordering::Relaxed) << 8 | u64::from(priority);
    let mut held = OWNER.load(Ordering::Acquire);
    loop {
        if held != 0 && held & 0xff >= u64::from(priority) {
            return None;
        }
        match OWNER.compare_exchange(held, token, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => return Some(Clock { token, last: now() }),
            Err(now_held) => held = now_held,
        }
    }
}

impl Drop for Clock {
    fn drop(&mut self) {
        // Free it, unless another took it.
        let _ = OWNER.compare_exchange(self.token, 0, Ordering::AcqRel, Ordering::Relaxed);
    }
}

impl Clock {
    /// Whether it still holds the clock.
    pub fn holds(&self) -> bool {
        OWNER.load(Ordering::Acquire) == self.token
    }

    /// Ends the frame of `tick`: every thread's spans that ended since the last one.
    pub fn end_tick(&mut self, tick: u64) {
        let end = now();
        let start = std::mem::replace(&mut self.last, end);
        if !enabled() || !self.holds() {
            // Spans recorded as profiling stopped are dropped with the next frame's.
            return;
        }
        let mut frame = Frame {
            tick,
            duration: end - start.min(end),
            ..Frame::default()
        };
        let buffers: Vec<Arc<Buffer>> = lock(&shared().buffers).clone();
        for buffer in buffers {
            let spans = std::mem::take(&mut *lock(&buffer.spans));
            if spans.is_empty() {
                continue;
            }
            let thread = index_of(&mut frame.threads, &buffer.thread);
            for raw in spans {
                let name = index_of(&mut frame.names, raw.name);
                frame.spans.push(Span {
                    name,
                    thread,
                    depth: raw.depth,
                    start: raw.start.saturating_sub(start),
                    end: raw.end.saturating_sub(start),
                });
            }
        }
        let mut frames = lock(&shared().frames);
        let number = frames.back().map_or(1, |(n, _)| n + 1);
        if frames.len() == KEPT {
            frames.pop_front();
        }
        frames.push_back((number, frame));
    }
}

fn index_of(list: &mut Vec<String>, name: &str) -> u16 {
    let at = list.iter().position(|n| n == name).unwrap_or_else(|| {
        list.push(name.to_owned());
        list.len() - 1
    });
    u16::try_from(at).unwrap_or(u16::MAX)
}

/// The frames kept, oldest first.
pub fn frames() -> Vec<Frame> {
    lock(&shared().frames)
        .iter()
        .map(|(_, f)| f.clone())
        .collect()
}

/// The frames made since `seen` (oldest first), and `seen` moved on past them: what a
/// watcher has not had yet (`seen` nought: every frame kept).
pub fn frames_since(seen: &mut u64) -> Vec<Frame> {
    let frames = lock(&shared().frames);
    let new: Vec<Frame> = frames
        .iter()
        .filter(|(n, _)| *n > *seen)
        .map(|(_, f)| f.clone())
        .collect();
    if let Some((n, _)) = frames.back() {
        *seen = (*seen).max(*n);
    }
    new
}
