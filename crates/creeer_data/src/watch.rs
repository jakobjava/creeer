//! Noticing that a pack changed: the files' sizes and modification times, polled.
//!
//! Polling twice a second needs no platform API and behaves the same on every OS; a walk of
//! the packs' few hundred files takes well under a millisecond.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, UNIX_EPOCH};

/// How often [`Watcher::poll`] looks.
pub const INTERVAL: Duration = Duration::from_millis(500);

/// Watches some directories for any change of a file in them.
#[derive(Clone, Debug)]
pub struct Watcher {
    roots: Vec<PathBuf>,
    stamp: u64,
    looked: Instant,
}

impl Watcher {
    /// Watches `roots` (and everything under them) from now on.
    pub fn new(roots: Vec<PathBuf>) -> Self {
        let stamp = stamp(&roots);
        Self {
            roots,
            stamp,
            looked: Instant::now(),
        }
    }

    /// Whether anything changed since the last change it reported, looking at most every
    /// [`INTERVAL`].
    pub fn poll(&mut self) -> bool {
        if self.looked.elapsed() < INTERVAL {
            return false;
        }
        self.check()
    }

    /// Whether anything changed since the last change it reported, looking now.
    pub fn check(&mut self) -> bool {
        self.looked = Instant::now();
        let now = stamp(&self.roots);
        let changed = now != self.stamp;
        self.stamp = now;
        changed
    }
}

/// A hash of every file's path, size and modification time under `roots`.
fn stamp(roots: &[PathBuf]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325_u64;
    for root in roots {
        walk(root, &mut h);
    }
    h
}

fn walk(dir: &Path, h: &mut u64) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        let Ok(meta) = std::fs::metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            walk(&path, h);
            continue;
        }
        let time = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_nanos());
        let mut eat = |bytes: &[u8]| {
            for &b in bytes {
                *h = (*h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
            }
        };
        eat(path.to_string_lossy().as_bytes());
        eat(&meta.len().to_le_bytes());
        eat(&time.to_le_bytes());
    }
}
