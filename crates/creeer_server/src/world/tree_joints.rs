//! Fastened to standing trees (G2.9, ADR-0033): a part (or a built structure) touching a
//! standing tree's trunk is lashed, pegged or nailed to it as to another part. The tree is the
//! other member: its wood the species' living wood, as thick as its stem there, its axis where
//! the joint runs to. It does not bend (a limit): the joint is held fast to the world on the
//! tree's axis (`Structure::fix_to`), as a post's soil springs are to the ground beside it.
//!
//! So nothing about it is kept beyond the structure: a node held fast on a tree's axis is a
//! joint to that tree, through saves, pieces falling apart and nodes put in. When the tree is
//! felled its joints above the cut let go, and what they held falls.

use super::building::{NEAR, TOUCHING};
use super::*;
use creeer_building::fasteners::{Fastener, side};
use creeer_building::law::Placement;
use creeer_physics::structure::{ImplicitSolver, Scheme};
use creeer_sim::trees::felled::BOX;
use creeer_sim::trees::{Standing, Tree};

/// How near a tree's axis a node held fast must lie to be fixed to it (m): it is put there and
/// never moves, so only rounding stands between.
const ON_AXIS: f64 = 1e-4;
/// How far out from its axis a point may lie and be on a trunk, over the radius of its box
/// (a square of the disc's area: its corners √2·√π/2 = 1.253 radii out), and beyond (m).
const OUT: f64 = 1.26;
const SLACK: f64 = 0.02;
/// A touch on a trunk is across it: its normal within 60° of level in the tree's frame (the
/// ground's is along the stem).
const ACROSS: f64 = 0.5;
/// How far about a point the trees whose trunks may reach it are looked for (m): the cells go
/// by direction from the planet's centre, and a stem leans a little.
const ABOUT: f64 = 2.0;

/// What a part touches to be fastened to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Touched {
    Part(EntityId),
    Tree(u64),
}

/// The points of segments `a` and `b` nearest each other (Ericson, *Real-Time Collision
/// Detection*, 5.1.9).
fn nearest(a: (DVec3, DVec3), b: (DVec3, DVec3)) -> (DVec3, DVec3) {
    let (d1, d2) = (a.1 - a.0, b.1 - b.0);
    let r = a.0 - b.0;
    let (aa, ee, f) = (d1.length_squared(), d2.length_squared(), d2.dot(r));
    let (c, bb) = (d1.dot(r), d1.dot(d2));
    let denominator = aa * ee - bb * bb;
    // Parallel: any point of a will do; its first.
    let mut s = if denominator > 1e-12 * aa * ee {
        ((bb * f - c * ee) / denominator).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mut t = (bb * s + f) / ee;
    if t < 0.0 {
        t = 0.0;
        s = (-c / aa).clamp(0.0, 1.0);
    } else if t > 1.0 {
        t = 1.0;
        s = ((bb - c) / aa).clamp(0.0, 1.0);
    }
    (a.0 + d1 * s, b.0 + d2 * t)
}

/// The height up standing tree `t`'s axis of point `p`, if `p` lies on it.
fn on_axis(t: &Standing, p: DVec3) -> Option<f64> {
    let local = t.turn.conjugate() * (p - t.base);
    (local.truncate().length() < ON_AXIS && local.z > 0.0).then_some(local.z)
}

impl ServerWorld {
    /// The trees (standing or felled to their stumps) whose cells lie about `p`.
    fn trees_about(&self, p: DVec3) -> Vec<Standing> {
        let Some(forest) = self.trees.forest.as_ref() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for cell in forest.cells_near(p, ABOUT) {
            match self.trees.cells.get(&cell) {
                Some(trees) => out.extend_from_slice(trees),
                None => out.extend(forest.in_cell(&self.terrain, cell)),
            }
        }
        out
    }

    /// The standing tree whose trunk point `p` is on (its boxes as laid, or its round stem as
    /// drawn), above its foot: the tree and its stem.
    pub(super) fn trunk_at(&self, p: DVec3) -> Option<(Standing, Tree)> {
        let forest = self.trees.forest.as_ref()?;
        (self.trees_about(p).into_iter())
            .filter(|t| !self.trees.felled.contains_key(&t.id))
            .filter_map(|t| {
                let stem = Tree::stem_only(&forest.species[usize::from(t.species)], t.dbh);
                let local = t.turn.conjugate() * (p - t.base);
                if !(0.05..stem.height).contains(&local.z) {
                    return None;
                }
                // Its box's radius: the stem's at the box's foot.
                let r = stem.radius((local.z / BOX).floor() * BOX);
                let across = local.truncate().length();
                (across <= OUT * r + SLACK).then_some((t, stem, across))
            })
            .min_by(|a, b| a.2.total_cmp(&b.2))
            .map(|(t, stem, _)| (t, stem))
    }

    /// The standing tree's trunk part `part` touches nearest `at`, and where: the point of the
    /// round stem facing the part's axis where it comes nearest the stem's. (A touch's own point
    /// is no guide: a pole's side face leaning on a trunk's box presses at its patch's middle,
    /// found 0.8 m down from the pole's top that touched.)
    pub(super) fn touching_tree(&self, part: EntityId, at: DVec3) -> Result<(u64, DVec3), String> {
        let i = self.index(part).ok_or("that part is gone")?;
        let axes = self.part_axes(part)?;
        let mut trees: Vec<(Standing, Tree)> = Vec::new();
        for t in self
            .world
            .touching(i, TOUCHING)
            .iter()
            .filter(|t| t.other.is_none())
        {
            if let Some((tree, stem)) = self.trunk_at(t.point)
                && t.normal.dot(tree.turn * DVec3::Z).abs() < ACROSS
                && trees.iter().all(|(s, _)| s.id != tree.id)
            {
                trees.push((tree, stem));
            }
        }
        let mut best: Option<(u64, DVec3, f64)> = None;
        for (tree, stem) in &trees {
            let up = tree.turn * DVec3::Z;
            let stem_axis = (tree.base + up * 0.05, tree.base + up * stem.height);
            for &part_axis in &axes {
                let (p, q) = nearest(part_axis, stem_axis);
                let r = stem.radius((q - tree.base).dot(up));
                let on = q + (p - q).normalize_or_zero() * r;
                let far = (on - at).length();
                if far < NEAR && best.is_none_or(|b| far < b.2) {
                    best = Some((tree.id, on, far));
                }
            }
        }
        best.map(|(id, on, _)| (id, on))
            .ok_or_else(|| "nothing touches it there to fasten to".into())
    }

    /// The axes of part `part`'s members (one, if it is not built yet).
    fn part_axes(&self, part: EntityId) -> Result<Vec<(DVec3, DVec3)>, String> {
        if let Some(g) = self.part_geometry(part)? {
            return Ok(vec![(g.part.start, g.part.end)]);
        }
        let s = &self.world.bodies[self.index(part).ok_or("it is gone")?].structure;
        Ok((0..s.element_count())
            .filter_map(|e| s.member(e))
            .map(|(a, b, _)| (s.position[a], s.position[b]))
            .collect())
    }

    /// What part `part` touches nearest `at` to be fastened to (another part or a standing
    /// tree), and the point between them.
    pub(super) fn touching_any(
        &self,
        part: EntityId,
        at: DVec3,
    ) -> Result<(Touched, DVec3), String> {
        let near = |p: DVec3| (p - at).length();
        match (self.touching_part(part, at), self.touching_tree(part, at)) {
            (Ok((_, p)), Ok((tree, q))) if near(q) < near(p) => Ok((Touched::Tree(tree), q)),
            (Ok((id, p)), _) => Ok((Touched::Part(id), p)),
            (Err(_), Ok((tree, q))) => Ok((Touched::Tree(tree), q)),
            (Err(e), Err(_)) => Err(e),
        }
    }

    /// The part touching the standing tree's trunk at `at` nearest it (G2.9: the crosshair on
    /// the trunk, not the part).
    pub(super) fn part_at_trunk(&self, at: DVec3) -> Result<EntityId, String> {
        let (tree, _) = self.trunk_at(at).ok_or("no tree's trunk there")?;
        (self.world.bodies.iter().enumerate())
            .filter(|(i, b)| {
                self.players[*i].is_none()
                    && (b.shapes.iter()).any(|(n, sh)| {
                        (b.structure.position[*n] - at).length() < sh.bounding_radius() + NEAR
                    })
            })
            .filter_map(|(i, _)| {
                let id = self.ids[i];
                let (t, q) = self.touching_tree(id, at).ok()?;
                (t == tree.id).then(|| (id, (q - at).length()))
            })
            .min_by(|x, y| x.1.total_cmp(&y.1))
            .map(|(id, _)| id)
            .ok_or_else(|| "nothing touches the trunk there to fasten".into())
    }

    /// Fastens part `part` to standing tree `tree` where it touches it at `point`, with one of
    /// `fastener` driven through the part into the tree (or a lashing round both): the joint
    /// from the part's member there to the tree's axis, held fast to the world.
    pub(super) fn fasten_to_tree(
        &mut self,
        part: EntityId,
        tree: u64,
        point: DVec3,
        fastener: &Fastener,
    ) -> Result<(), String> {
        let (node, axis, law) = self.tree_joint(part, tree, point, fastener)?;
        let i = self.index(part).ok_or("it is gone")?;
        let body = &mut self.world.bodies[i];
        body.structure.fix_to(node, axis, &law);
        body.solver = ImplicitSolver::new(&body.structure, Scheme::Newmark, DT, 1);
        body.wake();
        if let Some(r) = self.building.built.get_mut(&part) {
            r.woods
                .resize(self.world.bodies[i].structure.element_count(), None);
        }
        Ok(())
    }

    /// Where a joint of `fastener` from part `part` (made a structure) to standing tree `tree`
    /// at `point` goes: the part's node there, the point on the tree's axis it runs to, its law.
    pub(super) fn tree_joint(
        &mut self,
        part: EntityId,
        tree: u64,
        point: DVec3,
        fastener: &Fastener,
    ) -> Result<(usize, DVec3, creeer_physics::structure::JointLaw), String> {
        let (standing, _) = self.standing(tree).ok_or("that tree is felled")?;
        let forest = Arc::clone(self.trees.forest.as_ref().ok_or("no trees")?);
        let species = &forest.species[usize::from(standing.species)];
        let stem = Tree::stem_only(species, standing.dbh);
        self.make_built(part)?;
        let i = self.index(part).ok_or("it is gone")?;
        let count = self.world.bodies[i].structure.element_count();
        let (node, e, thick) = self.node_at(part, point, 0..count)?;
        let p = self.world.bodies[i].structure.position[node];
        let z = (standing.turn.conjugate() * (p - standing.base)).z;
        let trunk = 2.0 * stem.radius(z);
        if !(0.05..stem.height).contains(&z) || trunk <= 0.0 {
            return Err("not beside its stem".into());
        }
        let axis = standing.base + standing.turn * DVec3::new(0.0, 0.0, z);
        let between = (p - axis).length();
        let record = self.building.built.get(&part).ok_or("no record")?;
        let mine = (record.woods.get(e).cloned().flatten()).ok_or("a joint is no part")?;
        let sa = side(self.wood(&mine)?, thick)?;
        // The living wood, as thick as the stem along the fastener (a lashing goes round it).
        let sb = side(self.wood(&species.wood)?, trunk)?;
        let at = Placement {
            count: 1,
            spread: 0.0,
            contact: thick.min(trunk) * thick.min(trunk),
        };
        let materials = self.building.materials.as_ref().ok_or("no materials")?;
        let law = fastener.law(materials, (&sa, &sb), &at, between)?;
        Ok((node, axis, law))
    }

    /// The tree (standing or a stump) a node held fast at `p` is fixed to, if any.
    pub(super) fn tree_holding(&self, p: DVec3) -> Option<u64> {
        (self.trees_about(p).iter())
            .find(|t| on_axis(t, p).is_some())
            .map(|t| t.id)
    }

    /// The built structure with a joint fixed to the standing tree whose trunk `at` is on, the
    /// part's end of that joint nearest `at`.
    pub(super) fn joint_to_tree_near(&self, at: DVec3) -> Result<EntityId, String> {
        let (tree, _) = self.trunk_at(at).ok_or("no tree's trunk there")?;
        (self.building.built.keys())
            .filter_map(|&id| {
                let s = &self.world.bodies[self.index(id)?].structure;
                (0..s.element_count())
                    .filter_map(|e| s.joint_ends(e))
                    .filter(|(a, _)| s.held_fast(*a) && on_axis(&tree, s.position[*a]).is_some())
                    .map(|(_, b)| (s.position[b] - at).length())
                    .fold(None, |m: Option<f64>, d| Some(m.map_or(d, |m| m.min(d))))
                    .map(|d| (id, d))
            })
            .min_by(|x, y| x.1.total_cmp(&y.1))
            .map(|(id, _)| id)
            .ok_or_else(|| "nothing is fastened to the tree there".into())
    }

    /// Tree `t` felled at `cut` (m up its stem): the joints fixed to it above the cut let go
    /// (what they held falls; those below hold to its stump).
    pub(super) fn let_go_of_tree(&mut self, t: &Standing, cut: f64) {
        let built: Vec<EntityId> = self.building.built.keys().copied().collect();
        for id in built {
            let Some(i) = self.index(id) else { continue };
            let s = &self.world.bodies[i].structure;
            let held: Vec<usize> = (0..s.element_count())
                .filter(|&e| {
                    s.joint_ends(e).is_some_and(|(a, _)| {
                        s.held_fast(a) && on_axis(t, s.position[a]).is_some_and(|z| z > cut)
                    })
                })
                .collect();
            if held.is_empty() {
                continue;
            }
            let body = &mut self.world.bodies[i];
            for e in held {
                body.structure.sever(e);
            }
            body.solver.refactor(&body.structure);
            body.wake();
        }
    }
}
