//! The trees about the player, as this client grows them (G1.7, ADR-0031): the same forest as
//! the server's, cell by cell near the camera, less what the server says was felled (their
//! stumps stand). Drawn plainly, as the plan has it: stems and branches as rods in the bark's
//! colour, the crown as rough masses in the foliage's, finer the nearer (branch by branch, then
//! a few layers, then one mass). Felled trees' parts are drawn the same from what the server
//! tells of them. Aimed at by the crosshair's ray against their stems; their trunks are laid as
//! the server lays them, for the avatar's prediction.

use crate::physics_sandbox::PhysicsSnapshot;
use crate::tree_draw::{branches, crown, rods};
use creeer_math::glam::DVec3;
use creeer_net::entity::Kind;
use creeer_physics::world::shape::StaticShape;
use creeer_sim::scatter::Cell;
use creeer_sim::trees::{Forest, Part, Standing, Tree};
use creeer_terrain::Terrain;
use std::collections::BTreeMap;
use std::sync::Arc;

/// How far from the camera trees are drawn (m), with their branches, and in layers.
const DRAWN: f64 = 160.0;
const BRANCHES: f64 = 10.0;
const LAYERS: f64 = 50.0;
/// The camera moves this far before the cells are gathered again (m).
const REGATHER: f64 = 10.0;
/// Beyond `LAYERS`, stems thinner than this (m) are not drawn.
const FAR_STEMS: f64 = 0.15;
/// A stem drawn near is this many rods.
const SEGMENTS: usize = 6;

/// The trees near the camera.
#[derive(Debug)]
pub struct TreeView {
    forest: Arc<Forest>,
    cells: BTreeMap<Cell, Vec<Standing>>,
    /// Where they were gathered about, and the edits they were made with.
    centre: DVec3,
    edits: (u64, usize),
    /// Grown, those near the camera.
    grown: BTreeMap<u64, Tree>,
    /// The trees the felled parts in view are of, grown once each (by species, seed and
    /// girth): grown anew every frame for every part, they took a tenth of a millisecond each.
    felled: BTreeMap<(u16, u64, u64), Tree>,
    /// Where the camera was last.
    eye: DVec3,
}

/// The tree felled part `p` is of, as `TreeView::felled` keeps it.
fn felled_key(p: &creeer_net::entity::TreePart) -> (u16, u64, u64) {
    (p.species, p.seed, p.dbh.to_bits())
}

impl TreeView {
    /// For `terrain`, from the packs; none if they have no trees.
    pub fn new(terrain: &Terrain) -> Option<Self> {
        let spawn = creeer_server::world::spawn_point(terrain);
        let clearing = creeer_server::world::trees::SPAWN_CLEARING;
        let forest = Forest::from_data(&crate::content::data(), terrain).ok()?;
        let forest = forest.with_clearing(spawn, clearing);
        Some(Self {
            forest: Arc::new(forest),
            cells: BTreeMap::new(),
            centre: DVec3::ZERO,
            edits: (u64::MAX, 0),
            grown: BTreeMap::new(),
            felled: BTreeMap::new(),
            eye: DVec3::ZERO,
        })
    }

    /// Keeps grown the trees of the felled parts `c` knows, and only those.
    pub fn keep_felled(&mut self, c: &creeer_server::Connection) {
        let parts: Vec<&creeer_net::entity::TreePart> = (c.kinds.values())
            .filter_map(|k| match k {
                Kind::Tree(p) => Some(p),
                _ => None,
            })
            .collect();
        let keys: std::collections::BTreeSet<_> = parts.iter().map(|p| felled_key(p)).collect();
        self.felled.retain(|k, _| keys.contains(k));
        for p in parts {
            if let Some(species) = self.forest.species.get(usize::from(p.species)) {
                (self.felled.entry(felled_key(p)))
                    .or_insert_with(|| Tree::grow(species, p.seed, p.dbh));
            }
        }
    }

    /// Keeps the cells about the camera at `eye` (made anew where the edits changed), and the
    /// trees near it grown.
    pub fn update(&mut self, terrain: &Terrain, eye: DVec3, edits: (u64, usize)) {
        self.eye = eye;
        if edits != self.edits {
            self.cells.clear();
            self.edits = edits;
        }
        if self.cells.is_empty() || (eye - self.centre).length() > REGATHER {
            self.centre = eye;
            let near = self.forest.cells_near(eye, DRAWN + REGATHER);
            let keep: std::collections::BTreeSet<Cell> = near.iter().copied().collect();
            self.cells.retain(|c, _| keep.contains(c));
            for cell in near {
                (self.cells.entry(cell)).or_insert_with(|| self.forest.in_cell(terrain, cell));
            }
        }
        let forest = &self.forest;
        let near = |t: &Standing| (t.base - eye).length() <= LAYERS;
        let standing: Vec<&Standing> = self.cells.values().flatten().filter(|t| near(t)).collect();
        self.grown
            .retain(|id, _| standing.iter().any(|t| t.id == *id));
        for t in standing {
            self.grown.entry(t.id).or_insert_with(|| forest.grow(t));
        }
    }

    /// The trees standing (and the felled ones' stumps, as `c` was told) into `out`, seen from
    /// where the camera was last, and the felled trees' parts in view.
    pub fn draw(&self, out: &mut PhysicsSnapshot, c: &creeer_server::Connection) {
        let eye = self.eye;
        for t in self.cells.values().flatten() {
            let d = (t.base - eye).length();
            if d > DRAWN {
                continue;
            }
            let species = &self.forest.species[usize::from(t.species)];
            let felled = c.trees.get(&t.id).filter(|cut| cut.felled).map(|cut| cut.z);
            let stem = Tree::stem_only(species, t.dbh);
            let tree = self.grown.get(&t.id).unwrap_or(&stem);
            let to = felled.unwrap_or(tree.height);
            let place = |p: DVec3| t.base + t.turn * p;
            // Far, a thin stem is under a pixel: its crown alone.
            if d < LAYERS || t.dbh > FAR_STEMS || felled.is_some() {
                let n = if d < BRANCHES { SEGMENTS } else { 1 };
                rods(out, tree, (0.0, to), place, species.bark, n);
            }
            if felled.is_some() {
                continue;
            }
            if d < BRANCHES && !tree.nodes.is_empty() {
                let all = Part {
                    species: t.species,
                    seed: t.seed,
                    dbh: t.dbh,
                    from: 0.0,
                    to: tree.height,
                    cut: Vec::new(),
                };
                branches(
                    out,
                    tree,
                    &all,
                    (place, t.turn),
                    (species.bark, species.leaves),
                );
            } else {
                crown(
                    out,
                    &stem,
                    species,
                    (place, t.turn),
                    if d < LAYERS { 3 } else { 1 },
                );
            }
        }
        let poses = c.poses();
        for (id, kind) in &c.kinds {
            let (Kind::Tree(p), Some(pose)) = (kind, poses.get(id).copied()) else {
                continue;
            };
            let Some(species) = self.forest.species.get(usize::from(p.species)) else {
                continue;
            };
            let part = Part {
                species: p.species,
                seed: p.seed,
                dbh: p.dbh,
                from: p.from,
                to: p.to,
                cut: p.cut.clone(),
            };
            let grown;
            let tree = match self.felled.get(&felled_key(p)) {
                Some(t) => t,
                None => {
                    grown = Tree::grow(species, p.seed, p.dbh);
                    &grown
                }
            };
            let (centre, turn) = (p.centre, pose.orientation);
            let place = |q: DVec3| pose.position + turn * (q - centre);
            rods(out, tree, (p.from, p.to), place, species.bark, SEGMENTS);
            branches(
                out,
                tree,
                &part,
                (place, turn),
                (species.bark, species.leaves),
            );
        }
    }

    /// The standing tree the ray from `origin` along unit `dir` meets first within `length`
    /// (m): its id, the height up its stem and how far along the ray.
    pub fn aim(
        &self,
        c: &creeer_server::Connection,
        origin: DVec3,
        dir: DVec3,
        length: f64,
    ) -> Option<(Standing, f64, f64)> {
        let mut best: Option<(Standing, f64, f64)> = None;
        for t in self.cells.values().flatten() {
            if (t.base - origin).length() > length + 30.0 {
                continue;
            }
            let species = &self.forest.species[usize::from(t.species)];
            let stem = Tree::stem_only(species, t.dbh);
            let top =
                (c.trees.get(&t.id).filter(|cut| cut.felled)).map_or(stem.height, |cut| cut.z);
            // In the tree's frame: the ray against the stem's radius at the height it reaches
            // (twice, the second at the first's height).
            let (o, d) = (
                t.turn.conjugate() * (origin - t.base),
                t.turn.conjugate() * dir,
            );
            let mut hit = None;
            let mut r = stem.radius(1.0);
            for _ in 0..2 {
                let (a, b, cc) = (
                    d.x * d.x + d.y * d.y,
                    o.x * d.x + o.y * d.y,
                    o.x * o.x + o.y * o.y - r * r,
                );
                let disc = b * b - a * cc;
                if a < 1e-12 || disc < 0.0 {
                    hit = None;
                    break;
                }
                let s = (-b - disc.sqrt()) / a;
                let z = o.z + d.z * s;
                hit = Some((s, z));
                r = stem.radius(z.clamp(0.0, stem.height));
            }
            if let Some((s, z)) = hit
                && s >= 0.0
                && s <= length
                && (0.0..top).contains(&z)
                && best.is_none_or(|b| s < b.2)
            {
                best = Some((*t, z, s));
            }
        }
        best
    }

    /// Standing tree `id`, if its cell is about the camera; and the forest.
    pub fn standing(&self, id: u64) -> Option<(Standing, &Forest)> {
        let t = self.cells.values().flatten().find(|t| t.id == id)?;
        Some((*t, &self.forest))
    }

    /// A word for standing tree `t` (its girth, and how far it is cut), never its species: a
    /// spruce and a pine are the player's to tell apart (principle 9).
    pub fn name(&self, c: &creeer_server::Connection, t: &Standing) -> String {
        let mut name = format!("tree, {:.0} cm thick", t.dbh * 100.0);
        if let Some(cut) = c.trees.get(&t.id) {
            let species = &self.forest.species[usize::from(t.species)];
            let r = Tree::stem_only(species, t.dbh).radius(cut.z);
            let deepest = cut.notches.iter().map(|n| n.1).fold(0.0, f64::max);
            name += &format!(" · notch {:.0} % through", 50.0 * deepest / r.max(1e-6));
        }
        name
    }

    /// The trunks within `radius` of `p` as boxes, the felled ones' stumps (as the server lays
    /// them for its world: the avatar's prediction meets the same).
    pub fn trunks(&self, c: &creeer_server::Connection, p: DVec3, radius: f64) -> Vec<StaticShape> {
        let felled: BTreeMap<u64, f64> = (c.trees.values())
            .filter(|t| t.felled)
            .map(|t| (t.id, t.z))
            .collect();
        let near: BTreeMap<u64, &Standing> = (self.forest.cells_near(p, radius).iter())
            .filter_map(|cell| self.cells.get(cell))
            .flatten()
            .filter(|t| (t.base - p).length() <= radius + 1.0)
            .map(|t| (t.id, t))
            .collect();
        creeer_server::world::trees::trunk_shapes(&self.forest, near.into_values(), &felled)
    }
}

impl crate::planet_view::PlanetView {
    /// The trees about the camera at `camera`, and the trunks about the avatar for its
    /// prediction, as the server lays them (`world::trees::TRUNKS_NEAR_PLAYER`).
    pub fn update_trees(&mut self, camera: DVec3) {
        let Some(trees) = self.trees.as_mut() else {
            return;
        };
        let c = &self.session.connection;
        trees.update(
            &self.terrain,
            camera,
            (c.edits_generation, c.edits.log().len()),
        );
        trees.keep_felled(c);
        if let Some(avatar) = c.avatar() {
            let near = creeer_server::world::trees::TRUNKS_NEAR_PLAYER;
            let trunks = trees.trunks(c, avatar.position, near);
            self.session.connection.set_statics(trunks);
        }
    }
}
