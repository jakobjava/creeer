//! A lashing tacked, then finished (G2.10, ADR-0036): one hand holds a part where it is
//! wanted (steady) while the other makes the lashing's first turns, as a lone builder starts a
//! square lashing; those turns hold it once let go, and both hands then draw the rest, making
//! the joint the whole lashing's where its parts lie. The tack is told by what its joint keeps
//! (its turns' hold), so it is known after a save and wherever the structure goes.

use super::building::{UnderWay, Work};
use super::tree_joints::Touched;
use super::*;
use creeer_physics::structure::{ImplicitSolver, Scheme};

impl ServerWorld {
    /// What player `by`'s fastening of `part` at `at` with the catalogue's fastener `k` is:
    /// held, a part is nailed or pegged by the free hand while the other keeps it steady (H),
    /// or tacked by a lashing's first turns; hands free, a lashing tacked there is finished,
    /// anything else fastened whole. Refused unless the parts touch there.
    pub(super) fn fastening_work(
        &self,
        by: EntityId,
        part: EntityId,
        at: DVec3,
        k: usize,
    ) -> Result<Work, String> {
        let fastener = &self.building.fasteners[k];
        let lashing = matches!(
            fastener.kind,
            creeer_building::fasteners::Kind::Lashing { .. }
        );
        let tacked = lashing && self.tack_near(part, at, k).is_some();
        let Some(c) = self.carrying.get(&by) else {
            if tacked {
                return Ok(Work::Finish);
            }
            self.touching_any(part, at)?;
            return Ok(Work::Fasten);
        };
        if tacked {
            return Err("finishing a lashing takes both hands: let go first (G)".into());
        }
        if !c.tool && c.steady.is_none() {
            return Err("hold it steady first (H), then fasten it".into());
        }
        let (other, _) = self.touching_any(part, at)?;
        if !lashing {
            return Ok(Work::Fasten);
        }
        // One hand holds one of the parts in place; the other wraps (not round a tool in hand).
        let holds_one = c.load == part || matches!(other, Touched::Part(id) if id == c.load);
        if holds_one && !c.tool && fastener.tacked().is_some() {
            Ok(Work::Tack)
        } else {
            Err("a lashing takes both hands: put down what you hold".into())
        }
    }

    /// The joint of built structure `id` nearest `at` (within reach of it), if it is a tack of
    /// the catalogue's fastener `k` (a lashing).
    pub(super) fn tack_near(&self, id: EntityId, at: DVec3, k: usize) -> Option<usize> {
        let e = self.joint_near(id, at).ok()?;
        let kept = (self.world.bodies[self.index(id)?].structure).joint_withdrawal(e)?;
        let materials = self.building.materials.as_ref()?;
        let tacked = self.building.fasteners.get(k)?.tacked()?;
        let tack = tacked.lashing_withdrawal(materials).ok()??;
        ((kept / tack - 1.0).abs() < 1e-9).then_some(e)
    }

    /// A tack finished, its time up: if its maker still reaches it, its joint made anew with
    /// the whole lashing where its parts lie now (to a standing tree's stem, if it was).
    pub(super) fn finish_due(&mut self, by: EntityId, u: &UnderWay) -> Result<(), String> {
        let i = self.index(u.part).ok_or("it is gone")?;
        self.within_reach(by, u.at)?;
        let e = (self.tack_near(u.part, u.at, u.fastener)).ok_or("no tack there to finish")?;
        let whole = self.building.fasteners[u.fastener].clone();
        let s = &self.world.bodies[i].structure;
        let (a, b) = s.joint_ends(e).ok_or("no tack there to finish")?;
        let tree = (s.held_fast(a))
            .then(|| self.tree_holding(s.position[a]))
            .flatten();
        let pb = s.position[b];
        if let Some(tree) = tree {
            // To a tree: from the part's node to the stem's axis, as it was made.
            let (node, axis, law) = self.tree_joint(u.part, tree, pb, &whole)?;
            let i = self.index(u.part).ok_or("it is gone")?;
            let s = &mut self.world.bodies[i].structure;
            s.sever(e);
            s.fix_to(node, axis, &law);
        } else {
            let member = |n: usize| {
                (0..s.element_count())
                    .find_map(|m| {
                        s.member(m)
                            .filter(|(x, y, _)| *x == n || *y == n)
                            .map(|(_, _, t)| (n, m, t))
                    })
                    .ok_or("a joint is no part")
            };
            let ends = [member(a)?, member(b)?];
            let law = self.joint_law(u.part, ends, &whole)?;
            let s = &mut self.world.bodies[i].structure;
            s.sever(e);
            s.join(a, b, &law);
        }
        let i = self.index(u.part).ok_or("it is gone")?;
        let body = &mut self.world.bodies[i];
        body.solver = ImplicitSolver::new(&body.structure, Scheme::Newmark, DT, 1);
        body.wake();
        let count = body.structure.element_count();
        if let Some(r) = self.building.built.get_mut(&u.part) {
            r.woods.resize(count, None);
        }
        Ok(())
    }

    /// The hands that hold any of `parts` let go: fastened, the joint holds them (and a part
    /// made a structure has other nodes than the one gripped).
    pub(super) fn hands_off(&mut self, parts: &[EntityId]) {
        let holding: Vec<EntityId> = (self.carrying.iter())
            .filter(|(_, c)| parts.contains(&c.load))
            .map(|(by, _)| *by)
            .collect();
        for by in holding {
            self.let_go(by);
        }
    }
}
