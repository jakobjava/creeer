//! Simulation LOD for bodies (F7.2, ADR-0019): full physics near players, nothing far away.
//!
//! - **Active:** within [`ACTIVE`] of a player every body is simulated in full.
//! - **Parked:** an island of bodies (bodies whose bounding balls come within [`MARGIN`] of
//!   each other, so a stack goes as one) that is entirely asleep and farther than [`PARK`]
//!   from every player is taken out of the world and kept as it was ([`Parked`]); it costs
//!   nothing per tick, and is put back unchanged when a player comes within [`ACTIVE`].
//!   Asleep, its bodies are not stepped, and a sleeping body keeps no friction anchors or
//!   contact energies in the world (they are the last step's, of the bodies it stepped): so
//!   nothing of it is lost while it is away.
//! - An **awake** body is simulated wherever it is, until it comes to rest.
//! - **On rails** (the universe's vessels, ADR-0007) shares [`ACTIVE`] and [`PARK`].
//!
//! The hysteresis between the two radii keeps a player walking at the edge from parking and
//! waking the same bodies over and over.

use creeer_math::glam::DVec3;
use creeer_physics::world::{Body, World};
use std::collections::{BTreeMap, HashMap};

/// Bodies within this distance of a player are simulated (m).
pub const ACTIVE: f64 = 2_500.0;
/// Resting islands farther than this from every player are parked (m).
pub const PARK: f64 = 3_000.0;
/// Ticks between the passes that park and wake (a walking player moves a metre).
pub const PERIOD: u64 = 10;
/// The most bodies put back in one tick: a large parked yard comes back over several ticks,
/// nearest first, so arriving near it is no spike (a player reaches [`ACTIVE`] from it long
/// before it is in view).
pub const WAKE_PER_TICK: usize = 200;
/// Bodies whose bounding balls come this close are one island (m).
pub const MARGIN: f64 = 0.05;
/// The grid parked islands are found by (m).
const CELL: f64 = ACTIVE;
/// The grid islands are found in (m).
const NEAR_CELL: f64 = 4.0;

/// A ball containing every node and shape of `body`: (centre, radius).
pub fn bounds(body: &Body) -> (DVec3, f64) {
    let s = &body.structure;
    if let [p] = s.position[..] {
        // A rigid body: its shapes about its one node.
        let r = body.shapes.iter().map(|(_, shape)| shape.bounding_radius());
        let r = r.fold(0.0, f64::max);
        return (p, r);
    }
    let mut reach = vec![0.0f64; s.position.len()];
    for (node, shape) in &body.shapes {
        reach[*node] = reach[*node].max(shape.bounding_radius());
    }
    let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
    for (i, &p) in s.position.iter().enumerate() {
        let r = reach[i];
        lo = lo.min(p - DVec3::splat(r));
        hi = hi.max(p + DVec3::splat(r));
    }
    ((lo + hi) * 0.5, (hi - lo).length() * 0.5)
}

/// A ball containing `balls`.
fn enclosing(balls: &[(DVec3, f64)]) -> (DVec3, f64) {
    let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
    for &(c, r) in balls {
        lo = lo.min(c - DVec3::splat(r));
        hi = hi.max(c + DVec3::splat(r));
    }
    let centre = (lo + hi) * 0.5;
    let radius = balls
        .iter()
        .map(|&(c, r)| (c - centre).length() + r)
        .fold(0.0, f64::max);
    (centre, radius)
}

fn cell(p: DVec3, size: f64) -> [i64; 3] {
    #[allow(clippy::cast_possible_truncation)] // planet-sized coordinates
    let k = |x: f64| (x / size).floor() as i64;
    [k(p.x), k(p.y), k(p.z)]
}

fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

/// The world's bodies in islands (each sorted, the islands by their first body), with each
/// island's ball. Bodies are joined when their balls come within [`MARGIN`].
pub fn islands(world: &World) -> Vec<(Vec<usize>, (DVec3, f64))> {
    let balls: Vec<(DVec3, f64)> = world.bodies.iter().map(bounds).collect();
    islands_of(&balls)
}

/// The islands of bodies with balls `balls`.
fn islands_of(balls: &[(DVec3, f64)]) -> Vec<(Vec<usize>, (DVec3, f64))> {
    let mut grid: HashMap<[i64; 3], Vec<usize>> = HashMap::new();
    for (i, &(c, r)) in balls.iter().enumerate() {
        let (lo, hi) = (
            cell(c - DVec3::splat(r), NEAR_CELL),
            cell(c + DVec3::splat(r), NEAR_CELL),
        );
        for x in lo[0]..=hi[0] {
            for y in lo[1]..=hi[1] {
                for z in lo[2]..=hi[2] {
                    grid.entry([x, y, z]).or_default().push(i);
                }
            }
        }
    }
    let mut parent: Vec<usize> = (0..balls.len()).collect();
    let mut cells: Vec<&Vec<usize>> = grid.values().collect();
    cells.sort_unstable(); // determinism: the order joins are made in
    for members in cells {
        for (k, &a) in members.iter().enumerate() {
            for &b in &members[k + 1..] {
                let (ca, ra) = balls[a];
                let (cb, rb) = balls[b];
                if (ca - cb).length() <= ra + rb + MARGIN {
                    let (x, y) = (find(&mut parent, a), find(&mut parent, b));
                    if x != y {
                        parent[x.max(y)] = x.min(y);
                    }
                }
            }
        }
    }
    let mut by_root: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..balls.len() {
        let root = find(&mut parent, i);
        by_root.entry(root).or_default().push(i);
    }
    by_root
        .into_values()
        .map(|members| {
            let ball = enclosing(&members.iter().map(|&i| balls[i]).collect::<Vec<_>>());
            (members, ball)
        })
        .collect()
}

/// Whether a ball lies farther than `distance` from every one of `centres`.
fn beyond(ball: (DVec3, f64), centres: &[DVec3], distance: f64) -> bool {
    centres
        .iter()
        .all(|&c| (c - ball.0).length() - ball.1 > distance)
}

/// The islands of `world` to park: entirely asleep, farther than [`PARK`] from every one of
/// `centres` (each its bodies, sorted).
pub fn to_park(world: &World, centres: &[DVec3]) -> Vec<Vec<usize>> {
    let balls: Vec<(DVec3, f64)> = world.bodies.iter().map(bounds).collect();
    // Usually every resting body is near a player: then there is nothing to find.
    let any = (balls.iter().zip(&world.bodies))
        .any(|(&ball, body)| body.sleep.asleep && beyond(ball, centres, PARK));
    if !any {
        return Vec::new();
    }
    islands_of(&balls)
        .into_iter()
        .filter(|(members, ball)| {
            beyond(*ball, centres, PARK) && members.iter().all(|&i| world.bodies[i].sleep.asleep)
        })
        .map(|(members, _)| members)
        .collect()
}

/// A parked island: its bodies as they were, each with what its owner keeps of it.
#[derive(Clone, Debug)]
pub struct Island<T> {
    pub bodies: Vec<(Body, T)>,
    /// Its ball.
    pub centre: DVec3,
    pub radius: f64,
}

impl<T> Island<T> {
    /// An island of `bodies` (its ball measured).
    pub fn new(bodies: Vec<(Body, T)>) -> Self {
        let balls: Vec<(DVec3, f64)> = bodies.iter().map(|(b, _)| bounds(b)).collect();
        let (centre, radius) = enclosing(&balls);
        Self {
            bodies,
            centre,
            radius,
        }
    }
}

/// The parked islands, found by where they are.
#[derive(Clone, Debug)]
pub struct Parked<T> {
    islands: BTreeMap<u64, Island<T>>,
    cells: HashMap<[i64; 3], Vec<u64>>,
    next: u64,
}

impl<T> Default for Parked<T> {
    fn default() -> Self {
        Self {
            islands: BTreeMap::new(),
            cells: HashMap::new(),
            next: 0,
        }
    }
}

impl<T> Parked<T> {
    /// Parks `island`; returns its key (keys grow: older islands first).
    pub fn park(&mut self, island: Island<T>) -> u64 {
        let key = self.next;
        self.next += 1;
        for c in cells_of(island.centre, island.radius) {
            self.cells.entry(c).or_default().push(key);
        }
        self.islands.insert(key, island);
        key
    }

    /// The islands that come within `distance` of any of `centres`, nearest first (ties:
    /// oldest first).
    pub fn near(&self, centres: &[DVec3], distance: f64) -> Vec<u64> {
        let mut found: Vec<(f64, u64)> = Vec::new();
        for &c in centres {
            for keys in cells_of(c, distance).filter_map(|k| self.cells.get(&k)) {
                for &k in keys {
                    let island = &self.islands[&k];
                    let gap = (c - island.centre).length() - island.radius;
                    if gap < distance {
                        found.push((gap, k));
                    }
                }
            }
        }
        found.sort_unstable_by(|a, b| a.1.cmp(&b.1).then(a.0.total_cmp(&b.0)));
        // Each island once, at its nearest gap.
        found.dedup_by_key(|f| f.1);
        found.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        found.into_iter().map(|(_, k)| k).collect()
    }

    /// How many bodies island `key` has (0 if it is not parked).
    pub fn size(&self, key: u64) -> usize {
        self.islands.get(&key).map_or(0, |i| i.bodies.len())
    }

    /// Takes island `key` out.
    pub fn take(&mut self, key: u64) -> Option<Island<T>> {
        let island = self.islands.remove(&key)?;
        for c in cells_of(island.centre, island.radius) {
            if let Some(keys) = self.cells.get_mut(&c) {
                keys.retain(|&k| k != key);
                if keys.is_empty() {
                    self.cells.remove(&c);
                }
            }
        }
        Some(island)
    }

    /// The key of the first island with a body whose tag `pick` chooses.
    pub fn find(&self, mut pick: impl FnMut(&T) -> bool) -> Option<u64> {
        (self.islands.iter())
            .find(|(_, island)| island.bodies.iter().any(|(_, t)| pick(t)))
            .map(|(&k, _)| k)
    }

    /// Every island, oldest first.
    pub fn iter(&self) -> impl Iterator<Item = &Island<T>> {
        self.islands.values()
    }

    /// How many islands and bodies are parked.
    pub fn counts(&self) -> (usize, usize) {
        let bodies = self.islands.values().map(|i| i.bodies.len()).sum();
        (self.islands.len(), bodies)
    }
}

/// The cells of the parked islands' grid a ball touches.
fn cells_of(centre: DVec3, radius: f64) -> impl Iterator<Item = [i64; 3]> {
    let r = DVec3::splat(radius);
    let (lo, hi) = (cell(centre - r, CELL), cell(centre + r, CELL));
    (lo[0]..=hi[0]).flat_map(move |x| {
        (lo[1]..=hi[1]).flat_map(move |y| (lo[2]..=hi[2]).map(move |z| [x, y, z]))
    })
}
