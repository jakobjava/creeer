//! Trees (G1.7, ADR-0031). Standing trees are scenery (`creeer_sim::trees`): every machine grows
//! the same from the planet's seed, and nothing of them is simulated until struck. The server
//! keeps what was done to them: cuts into standing trees, the trees felled (their stumps stand),
//! and felled trees' parts as rigid bodies (`felling.rs`, `bucking.rs`). The trunks near what
//! moves are boxes in the physics world's fixed geometry, refreshed every few ticks, so a
//! character walks into a tree and a falling one lands on a stump; clients lay the same about
//! their avatar for their prediction.

use super::*;
use creeer_net::message::TreeCut;
use creeer_physics::world::shape::StaticShape;
use creeer_sim::scatter::Cell;
use creeer_sim::trees::{Cut, Forest, Part, Standing, Tree, chop};
use std::collections::{BTreeMap, BTreeSet};

/// The clearing about the spawn, where no tree stands (m): a player starts in the open, and the
/// play scenes (within 11 m of it) keep their ground.
pub const SPAWN_CLEARING: f64 = 30.0;
/// Ticks between two layings of the trunks about what moves.
const TRUNKS_EVERY: u64 = 10;
/// Cells made a tick, at most (about 20 µs each on ground already generated).
const CELLS_PER_TICK: usize = 4;
/// How far about a player the trunks are laid (m), and about any other body awake, beyond its
/// own reach: farther than either goes in `TRUNKS_EVERY` ticks.
pub const TRUNKS_NEAR_PLAYER: f64 = 12.0;
const TRUNKS_NEAR_BODY: f64 = 4.0;

/// What the world keeps of its trees.
#[derive(Debug, Default)]
pub(crate) struct Trees {
    pub(super) forest: Option<Arc<Forest>>,
    /// Standing trees cut into, by id.
    pub(super) cuts: BTreeMap<u64, Cut>,
    /// Felled trees: the height they were cut at (their stumps stand to it).
    pub(super) felled: BTreeMap<u64, f64>,
    /// The trees that changed, in order: clients are told from it.
    pub(super) log: Vec<u64>,
    /// Bodies that are felled trees' parts.
    pub(super) parts: BTreeMap<EntityId, Part>,
    /// Parts still on their hinge (a ball joint to the world where the cut went over).
    pub(super) hinged: BTreeSet<EntityId>,
    /// How many of the world's fixed shapes (at their end) are trunks.
    pub(super) trunks: usize,
    /// A part's cut being chopped through (bucking), and the wood taken out of a node's
    /// branches so far (limbing).
    pub(super) bucks: BTreeMap<EntityId, Cut>,
    pub(super) limbing: BTreeMap<EntityId, (usize, f64)>,
    /// The trees of the cells about what moves, as made; the cells waiting to be (a few a tick);
    /// and how many of the terrain's edits they have seen.
    pub(super) cells: BTreeMap<Cell, Vec<Standing>>,
    pub(super) pending: BTreeSet<Cell>,
    pub(super) edits: usize,
}

impl Trees {
    /// The packs' forest for `terrain` (none if the packs have no trees).
    pub(super) fn from_data(data: &Data, terrain: &Terrain, spawn: DVec3) -> Self {
        Self {
            forest: (Forest::from_data(data, terrain).ok())
                .map(|f| Arc::new(f.with_clearing(spawn, SPAWN_CLEARING))),
            ..Self::default()
        }
    }
}

impl ServerWorld {
    /// The forest's species and placement, if the packs have trees.
    pub fn forest(&self) -> Option<&Arc<Forest>> {
        self.trees.forest.as_ref()
    }

    /// Tree `id`, if it stands (not felled), and grown.
    pub(super) fn standing(&self, id: u64) -> Option<(Standing, Tree)> {
        if self.trees.felled.contains_key(&id) {
            return None;
        }
        let forest = self.trees.forest.as_ref()?;
        let standing = forest.find(&self.terrain, id)?;
        Some((standing, forest.grow(&standing)))
    }

    /// After a step: hinges let go where the notch closed, cells near new edits made again, a
    /// few waiting cells made, and the trunks laid anew every few ticks.
    pub(super) fn trees_after_step(&mut self) {
        self.release_hinges();
        self.remake_edited_cells();
        self.make_cells(CELLS_PER_TICK);
        if self.tick.is_multiple_of(TRUNKS_EVERY) {
            self.lay_trunks();
        }
    }

    /// Up to `n` of the cells waiting, made (the first touch of new ground generates its
    /// terrain: about 200 cells at once, when eight players' surroundings came into reach
    /// together, took 9 ms).
    pub(super) fn make_cells(&mut self, n: usize) {
        let Some(forest) = self.trees.forest.clone() else {
            return;
        };
        for _ in 0..n {
            let Some(cell) = self.trees.pending.pop_first() else {
                return;
            };
            let trees = forest.in_cell(&self.terrain, cell);
            self.trees.cells.insert(cell, trees);
        }
    }

    /// The cells near edits made since last seen, made again at once (their trees stand on the
    /// ground, which may have moved).
    fn remake_edited_cells(&mut self) {
        let edits = self.terrain.edits();
        if edits.len() == self.trees.edits {
            return;
        }
        let new: Vec<(DVec3, DVec3)> = (edits.log().get(self.trees.edits..).unwrap_or_default())
            .iter()
            .map(|e| e.brush().bounds())
            .collect();
        self.trees.edits = edits.len();
        let Some(forest) = self.trees.forest.clone() else {
            return;
        };
        for (lo, hi) in new {
            let centre = 0.5 * (lo + hi);
            let reach = 0.5 * (hi - lo).length() + creeer_sim::trees::TREE_CELL;
            for cell in forest.cells_near(centre, reach) {
                if self.trees.cells.contains_key(&cell) {
                    let trees = forest.in_cell(&self.terrain, cell);
                    self.trees.cells.insert(cell, trees);
                }
            }
        }
    }

    /// The trunks about the players and every body awake, as boxes in the fixed geometry (the
    /// felled ones' stumps), in place of the last: of the cells made; those not yet are waiting.
    pub(super) fn lay_trunks(&mut self) {
        let kept = self.world.statics.len() - self.trees.trunks;
        self.world.statics.truncate(kept);
        let Some(forest) = self.trees.forest.clone() else {
            self.trees.trunks = 0;
            return;
        };
        let mut used = BTreeSet::new();
        let mut near: BTreeMap<u64, Standing> = BTreeMap::new();
        for (i, body) in self.world.bodies.iter().enumerate() {
            let p = body.structure.position[0];
            // About a player every cell; about any other body awake, its cells lightly sampled.
            let (reach, cells) = if self.players[i].is_some() {
                (TRUNKS_NEAR_PLAYER, forest.cells_near(p, TRUNKS_NEAR_PLAYER))
            } else if !body.sleep.asleep {
                let r = body.shapes.iter().map(|(_, s)| s.bounding_radius());
                let reach = r.fold(0.0, f64::max) + TRUNKS_NEAR_BODY;
                (reach, forest.cells_about(p, reach))
            } else {
                continue;
            };
            for cell in cells {
                used.insert(cell);
                let Some(trees) = self.trees.cells.get(&cell) else {
                    self.trees.pending.insert(cell);
                    continue;
                };
                for t in trees {
                    if (t.base - p).length() <= reach + 1.0 {
                        near.insert(t.id, *t);
                    }
                }
            }
        }
        self.trees.cells.retain(|c, _| used.contains(c));
        self.trees.pending.retain(|c| used.contains(c));
        let shapes = trunk_shapes(&forest, near.values(), &self.trees.felled);
        self.trees.trunks = shapes.len();
        self.world.statics.extend(shapes);
    }

    /// The trunks laid with every cell they need made now (a test's world, set up).
    #[cfg(test)]
    pub(super) fn lay_trunks_now(&mut self) {
        self.lay_trunks();
        self.make_cells(usize::MAX);
        self.lay_trunks();
    }

    /// Parts on their hinge that have leant past the notch's opening: the notch closed on
    /// them, and the hinge tore (ADR-0031 decision 6).
    fn release_hinges(&mut self) {
        let hinged: Vec<EntityId> = self.trees.hinged.iter().copied().collect();
        for id in hinged {
            let Some(i) = self.index(id) else {
                self.trees.hinged.remove(&id);
                continue;
            };
            let s = &self.world.bodies[i].structure;
            let along = s.orientation[0] * DVec3::Z;
            let up = character::plumb(&self.world, s.position[0]);
            if along.dot(up) > creeer_math::det::cos(chop::OPENING) {
                continue;
            }
            let joint = (self.world.joints().iter()).position(|j| j.a.0 == i && j.b.is_none());
            if let Some(k) = joint {
                self.world.remove_joint(k);
            }
            self.trees.hinged.remove(&id);
        }
    }

    /// The trees changed since the first `told` of the log, as clients are told them (each
    /// once, as it is now), and how long the log is now.
    pub fn trees_since(&self, told: usize) -> (Vec<TreeCut>, usize) {
        let mut seen = BTreeSet::new();
        let new = (self.trees.log.get(told..).unwrap_or_default().iter())
            .filter(|id| seen.insert(**id))
            .filter_map(|&id| self.tree_cut(id))
            .collect();
        (new, self.trees.log.len())
    }

    /// Tree `id`'s cut as clients are told it.
    fn tree_cut(&self, id: u64) -> Option<TreeCut> {
        if let Some(&z) = self.trees.felled.get(&id) {
            return Some(TreeCut {
                id,
                z,
                notches: Vec::new(),
                felled: true,
            });
        }
        let cut = self.trees.cuts.get(&id)?;
        let (_, tree) = self.standing(id)?;
        let notches = cut.notches(tree.radius(cut.z));
        Some(TreeCut {
            id,
            z: cut.z,
            notches: notches.iter().map(|n| (n.azimuth, n.depth)).collect(),
            felled: false,
        })
    }

    /// Takes body `id` out of the world (a tree's part replaced by its pieces).
    pub(super) fn remove_entity(&mut self, id: EntityId) -> Option<Body> {
        let i = self.index(id)?;
        let body = self.world.bodies[i].clone();
        let map = self.world.retain_bodies(|k, _| k != i);
        self.surroundings.retain(&map);
        (self.ids, self.players) = (kept(&map, &self.ids), kept(&map, &self.players));
        self.reindex();
        self.ages.by_entity.remove(&id);
        self.trees.parts.remove(&id);
        self.trees.hinged.remove(&id);
        self.trees.bucks.remove(&id);
        self.trees.limbing.remove(&id);
        self.world.wake_near(body.structure.position[0], 1.0);
        Some(body)
    }
}

/// The trunks of `trees` as boxes: whole where they stand, to their cut where `felled`.
pub fn trunk_shapes<'a>(
    forest: &Forest,
    trees: impl Iterator<Item = &'a Standing>,
    felled: &BTreeMap<u64, f64>,
) -> Vec<StaticShape> {
    let mut out = Vec::new();
    for t in trees {
        let species = &forest.species[usize::from(t.species)];
        let stem = Tree::stem_only(species, t.dbh);
        let up_to = felled.get(&t.id).copied().unwrap_or(stem.height);
        out.extend(stem.trunk_boxes(t.base, t.turn, up_to));
    }
    out
}

impl ServerWorld {
    /// Body `index` as a felled tree's part, as clients draw it: its tree, its span, its
    /// branches off, its centre of mass in the tree's frame and its hull about that.
    pub(super) fn tree_part(&self, index: usize) -> Option<creeer_net::entity::TreePart> {
        let part = self.trees.parts.get(&self.ids[index])?;
        let tree = self.trees.forest.as_ref()?.grow_part(part)?;
        let (_, centre, _) = tree.part_mass(part);
        let hull = match self.world.bodies[index].shapes.first() {
            Some((_, Shape::Hull(h))) => h.vertices().to_vec(),
            _ => Vec::new(),
        };
        Some(creeer_net::entity::TreePart {
            species: part.species,
            seed: part.seed,
            dbh: part.dbh,
            from: part.from,
            to: part.to,
            cut: part.cut.clone(),
            centre,
            hull,
        })
    }
}

impl ServerWorld {
    /// What was done to the trees, as a save keeps it.
    pub(super) fn saved_trees(&self) -> creeer_sim::save::SavedTrees {
        let t = &self.trees;
        creeer_sim::save::SavedTrees {
            cuts: t.cuts.iter().map(|(id, c)| (*id, c.clone())).collect(),
            felled: t.felled.iter().map(|(id, z)| (*id, *z)).collect(),
            parts: t.parts.iter().map(|(id, p)| (id.0, p.clone())).collect(),
            hinged: t.hinged.iter().map(|id| id.0).collect(),
            trunks: t.trunks as u64,
            bucks: t.bucks.iter().map(|(id, c)| (id.0, c.clone())).collect(),
            limbing: (t.limbing.iter())
                .map(|(id, (node, v))| (id.0, *node as u64, *v))
                .collect(),
            cells: t.cells.keys().map(|c| c.key()).collect(),
            pending: t.pending.iter().map(|c| c.key()).collect(),
            edits: t.edits as u64,
        }
    }

    /// What a save kept of the trees.
    pub(super) fn restore_trees(&mut self, saved: creeer_sim::save::SavedTrees) {
        let t = &mut self.trees;
        t.cuts = saved.cuts.into_iter().collect();
        t.felled = saved.felled.into_iter().collect();
        t.parts = (saved.parts.into_iter())
            .map(|(id, p)| (EntityId(id), p))
            .collect();
        t.hinged = saved.hinged.into_iter().map(EntityId).collect();
        #[allow(clippy::cast_possible_truncation)] // as many as were laid
        let trunks = saved.trunks as usize;
        t.trunks = trunks.min(self.world.statics.len());
        t.bucks = (saved.bucks.into_iter())
            .map(|(id, c)| (EntityId(id), c))
            .collect();
        t.limbing = (saved.limbing.into_iter())
            .map(|(id, node, v)| (EntityId(id), (usize::try_from(node).unwrap_or(0), v)))
            .collect();
        t.log = t.cuts.keys().chain(t.felled.keys()).copied().collect();
        t.pending = saved.pending.into_iter().map(Cell::from_key).collect();
        #[allow(clippy::cast_possible_truncation)] // as many as there were
        let edits = saved.edits as usize;
        t.edits = edits;
        // The cells made again: the same trees, from the same seed and ground.
        if let Some(forest) = t.forest.clone() {
            for key in saved.cells {
                let cell = Cell::from_key(key);
                let trees = forest.in_cell(&self.terrain, cell);
                self.trees.cells.insert(cell, trees);
            }
        }
    }
}

impl ServerWorld {
    /// Whether a thing reaching `radius` (m) across from `at` would be put down in a trunk
    /// (a stump's, to its cut): refused, as a player cannot put a crate into a tree.
    pub(super) fn clear_of_trunks(&self, at: DVec3, radius: f64) -> Result<(), String> {
        let Some(forest) = self.trees.forest.as_ref() else {
            return Ok(());
        };
        for cell in forest.cells_near(at, radius + 1.0) {
            let made;
            let trees = match self.trees.cells.get(&cell) {
                Some(t) => t,
                None => {
                    made = forest.in_cell(&self.terrain, cell);
                    &made
                }
            };
            for t in trees {
                let up = t.base.normalize();
                let off = at - t.base;
                let height = off.dot(up);
                let stem = Tree::stem_only(&forest.species[usize::from(t.species)], t.dbh);
                let top = self.trees.felled.get(&t.id).copied().unwrap_or(stem.height);
                let across = (off - up * height).length();
                let r = stem.radius(height.clamp(0.0, top));
                if height > -radius && height < top + radius && across < r + radius {
                    return Err("a tree stands there".into());
                }
            }
        }
        Ok(())
    }
}
