//! Terrain streaming: which chunks to show for a camera (level of detail), built on worker
//! threads so that the frame never waits for terrain (plan.md §5; ADR-0006).
//!
//! **Level of detail.** A chunk is split into its eight children while the camera is closer
//! to it than `detail` times its size, so its cells look about equally large on screen at
//! every distance. Chunks without surface (all air, all rock) are never split.
//!
//! **No holes, no waiting.** A chunk is shown in place of its children until all of them are
//! built, so refining never opens holes; zooming out shows the coarser parent at once if it is
//! still cached, or else its parent. Unknown chunks are queued; workers take the most important
//! first (coarse and near: smallest distance per size).
//!
//! **Edits** mark the chunks they touch as stale: the old mesh stays on screen until the new
//! one is built.

use crate::chunk::{ChunkKey, KeyMap, KeySet, MAX_CHUNK_LEVEL, Occupancy};
use crate::terrain::Terrain;
use creeer_math::glam::{DVec3, IVec3};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread::JoinHandle;

/// Tuning of the streamer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StreamSettings {
    /// A chunk splits while the camera is within this many chunk sizes of it.
    pub detail: f64,
    /// Worker threads.
    pub workers: usize,
    /// Finished chunks taken per update (bounds the GPU uploads per frame).
    pub results_per_update: usize,
    /// Chunks kept in memory beyond those on screen.
    pub cache: usize,
}

impl Default for StreamSettings {
    fn default() -> Self {
        let cores = std::thread::available_parallelism().map_or(4, std::num::NonZero::get);
        Self {
            detail: 2.5,
            // Leave cores for the simulation and render threads.
            workers: cores.saturating_sub(2).max(1),
            results_per_update: 64,
            cache: 3000,
        }
    }
}

/// What the renderer must do after an update.
#[derive(Debug)]
pub struct Update<T> {
    /// Chunks newly built (or rebuilt after an edit): upload them.
    pub ready: Vec<(ChunkKey, Arc<T>)>,
    /// Chunks dropped from the cache: free them.
    pub evicted: Vec<ChunkKey>,
    /// Chunks to draw this frame, each with the faces whose skirts it needs (bit f for face
    /// f as in `TerrainMesh::skirt_faces`): those bordering a chunk of another level.
    pub draw: Vec<(ChunkKey, u8)>,
}

struct Entry<T> {
    /// Whether the chunk may hold surface (and may therefore be split).
    mixed: bool,
    data: Option<Arc<T>>,
    stale: bool,
    used: u64,
}

type Built<T> = (ChunkKey, bool, Option<T>);

/// Work shared with the workers.
#[derive(Default)]
struct Work {
    /// Chunks wanted, most important first (replaced every update).
    pending: Vec<ChunkKey>,
    /// Chunks a worker is building (until the streamer receives them).
    taken: KeySet,
    stop: bool,
}

type Queue = (Mutex<Work>, Condvar);

/// Streams the chunks of one terrain, building each with a function run on worker threads.
pub struct Streamer<T: Send + Sync + 'static> {
    terrain: Arc<Terrain>,
    settings: StreamSettings,
    entries: KeyMap<Entry<T>>,
    /// Chunks being built (a copy of the workers' `taken`, made at each update).
    in_flight: KeySet,
    queue: Arc<Queue>,
    results: Receiver<Built<T>>,
    workers: Vec<JoinHandle<()>>,
    frame: u64,
    /// Chunks queued in the last update (still to build).
    pub queued: usize,
}

impl<T: Send + Sync + 'static> std::fmt::Debug for Streamer<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Streamer")
            .field("cached", &self.entries.len())
            .field("in_flight", &self.in_flight.len())
            .finish_non_exhaustive()
    }
}

impl<T: Send + Sync + 'static> Streamer<T> {
    /// A streamer whose workers build each chunk that may hold surface with `build`
    /// (`None`: nothing to draw).
    pub fn new(
        terrain: Arc<Terrain>,
        settings: StreamSettings,
        build: impl Fn(&Terrain, ChunkKey) -> Option<T> + Send + Sync + 'static,
    ) -> Self {
        let queue: Arc<Queue> = Arc::new((Mutex::new(Work::default()), Condvar::new()));
        let (send, results) = channel();
        let build = Arc::new(build);
        let workers = (0..settings.workers)
            .map(|i| {
                let (queue, send, build) = (Arc::clone(&queue), send.clone(), Arc::clone(&build));
                let terrain = Arc::clone(&terrain);
                std::thread::Builder::new()
                    .name(format!("terrain {i}"))
                    .spawn(move || work(&terrain, &queue, &send, &*build))
                    .expect("spawn a terrain worker")
            })
            .collect();
        Self {
            terrain,
            settings,
            entries: KeyMap::default(),
            in_flight: KeySet::default(),
            queue,
            results,
            workers,
            frame: 0,
            queued: 0,
        }
    }

    /// Number of chunks in memory.
    pub fn cached(&self) -> usize {
        self.entries.len()
    }

    /// Marks every chunk overlapping the box `lo..hi` (with one cell around it) as stale,
    /// to be rebuilt (after an edit there).
    pub fn invalidate(&mut self, lo: DVec3, hi: DVec3) {
        for (key, entry) in &mut self.entries {
            let cell = key.cell_size();
            let (klo, khi) = (key.origin(), key.origin() + DVec3::splat(key.size()));
            if klo.cmple(hi + cell).all() && khi.cmpge(lo - cell).all() {
                entry.stale = true;
            }
        }
    }

    /// Takes finished chunks, chooses the chunks to show from `camera` and queues the ones
    /// still missing.
    pub fn update(&mut self, camera: DVec3) -> Update<T> {
        self.frame += 1;
        let mut ready = Vec::new();
        let results: Vec<Built<T>> = self
            .results
            .try_iter()
            .take(self.settings.results_per_update)
            .collect();
        {
            let mut work = self.queue.0.lock().unwrap_or_else(PoisonError::into_inner);
            for (key, _, _) in &results {
                work.taken.remove(key);
            }
            self.in_flight.clone_from(&work.taken);
        }
        for (key, mixed, data) in results {
            let data = data.map(Arc::new);
            if let Some(d) = &data {
                ready.push((key, Arc::clone(d)));
            }
            let used = self.frame;
            self.entries.insert(
                key,
                Entry {
                    mixed,
                    data,
                    stale: false,
                    used,
                },
            );
        }
        let mut draw = Vec::new();
        let mut wanted = Vec::new();
        for root in roots(&self.terrain) {
            self.visit(root, camera, &mut draw, &mut wanted);
        }
        // Most important first: coarse and near (distance per size).
        wanted.sort_by(|a: &(f64, ChunkKey), b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        wanted.dedup_by_key(|w| w.1);
        self.queued = wanted.len();
        {
            let (lock, wake) = &*self.queue;
            let mut work = lock.lock().unwrap_or_else(PoisonError::into_inner);
            // A worker may have taken a chunk since the copy above: never queue it twice.
            let taken = &work.taken;
            let pending: Vec<ChunkKey> = wanted
                .into_iter()
                .map(|(_, k)| k)
                .filter(|k| !taken.contains(k))
                .collect();
            work.pending = pending;
            wake.notify_all();
        }
        let evicted = self.evict();
        let drawn: KeySet = draw.iter().copied().collect();
        let draw = draw
            .into_iter()
            .map(|key| (key, skirt_mask(&drawn, key)))
            .collect();
        Update {
            ready,
            evicted,
            draw,
        }
    }

    /// Visits `key`: what to draw for it (into `draw`) and what to build (into `wanted`).
    /// Returns whether its part of the surface can be drawn completely.
    fn visit(
        &mut self,
        key: ChunkKey,
        camera: DVec3,
        draw: &mut Vec<ChunkKey>,
        wanted: &mut Vec<(f64, ChunkKey)>,
    ) -> bool {
        let priority = key.distance_to(camera) / key.size();
        let frame = self.frame;
        let Some(entry) = self.entries.get_mut(&key) else {
            if !self.in_flight.contains(&key) {
                wanted.push((priority, key));
            }
            return false;
        };
        entry.used = frame;
        if entry.stale && !self.in_flight.contains(&key) {
            wanted.push((priority, key));
        }
        if !entry.mixed {
            return true;
        }
        let has_mesh = entry.data.is_some();
        let split = key.level > 0 && key.distance_to(camera) < self.settings.detail * key.size();
        if split {
            let mut children = Vec::new();
            let mut complete = true;
            for child in key.children() {
                complete &= self.visit(child, camera, &mut children, wanted);
            }
            if complete {
                draw.extend(children);
                return true;
            }
        }
        if has_mesh {
            draw.push(key);
        }
        true
    }

    /// Drops the least recently used chunks beyond the cache size (never one used in this
    /// update).
    fn evict(&mut self) -> Vec<ChunkKey> {
        let excess = self.entries.len().saturating_sub(self.settings.cache);
        if excess == 0 {
            return Vec::new();
        }
        let mut old: Vec<(u64, ChunkKey)> = self
            .entries
            .iter()
            .filter(|(_, e)| e.used < self.frame)
            .map(|(k, e)| (e.used, *k))
            .collect();
        old.sort_unstable();
        old.truncate(excess);
        for (_, key) in &old {
            self.entries.remove(key);
        }
        old.into_iter().map(|(_, k)| k).collect()
    }
}

impl<T: Send + Sync + 'static> Drop for Streamer<T> {
    fn drop(&mut self) {
        let (lock, wake) = &*self.queue;
        lock.lock().unwrap_or_else(PoisonError::into_inner).stop = true;
        wake.notify_all();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

/// The coarsest chunks: those of the top level around the planet.
fn roots(terrain: &Terrain) -> Vec<ChunkKey> {
    let size = f64::from(1u32 << MAX_CHUNK_LEVEL) * 32.0;
    let extent =
        terrain.planet().datum.equatorial_radius() + terrain.heights().elevation().max_height();
    #[allow(clippy::cast_possible_truncation)] // a handful of chunks
    let n = (extent / size).ceil() as i32;
    let mut out = Vec::new();
    for z in -n..n {
        for y in -n..n {
            for x in -n..n {
                out.push(ChunkKey {
                    level: MAX_CHUNK_LEVEL,
                    pos: IVec3::new(x, y, z),
                });
            }
        }
    }
    out
}

/// A worker: builds the most important queued chunk, until told to stop.
fn work<T>(
    terrain: &Terrain,
    queue: &Queue,
    send: &Sender<Built<T>>,
    build: &(dyn Fn(&Terrain, ChunkKey) -> Option<T> + Send + Sync),
) {
    let (lock, wake) = queue;
    loop {
        let key = {
            let mut work = lock.lock().unwrap_or_else(PoisonError::into_inner);
            loop {
                if work.stop {
                    return;
                }
                if !work.pending.is_empty() {
                    let key = work.pending.remove(0);
                    work.taken.insert(key);
                    break key;
                }
                work = wake.wait(work).unwrap_or_else(PoisonError::into_inner);
            }
        };
        let mixed = terrain.occupancy(key) == Occupancy::Mixed;
        let data = if mixed { build(terrain, key) } else { None };
        if send.send((key, mixed, data)).is_err() {
            return;
        }
    }
}

/// The faces of `key` that need skirts: those whose neighbour of the same level is not drawn,
/// so a coarser or finer chunk meets it there (or nothing does: then no surface crosses that
/// face and its skirt is empty).
fn skirt_mask(drawn: &KeySet, key: ChunkKey) -> u8 {
    let mut mask = 0u8;
    for face in 0..6 {
        let mut step = IVec3::ZERO;
        step[face / 2] = if face % 2 == 0 { -1 } else { 1 };
        let neighbour = ChunkKey {
            pos: key.pos + step,
            ..key
        };
        if !drawn.contains(&neighbour) {
            mask |= 1 << face;
        }
    }
    mask
}
