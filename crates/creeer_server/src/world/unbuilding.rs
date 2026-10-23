//! Taking apart (G2.6, ADR-0033): the joint nearest where the player points on a built
//! structure has its fasteners taken out (nails drawn, a peg knocked out, a lashing undone)
//! after the time it takes; what it held may then fall apart (`apart.rs`).

use super::building::UnderWay;
use super::*;

/// How long taking a joint apart takes by hand (s: an estimate, as fastening's).
const UNDOING: f64 = 10.0;
/// How far from the point aimed at a joint is found (m).
const NEAR: f64 = 0.2;

impl ServerWorld {
    /// Player `by` begins taking apart the joint of built structure `built` nearest `at`.
    pub(super) fn begin_unfastening(
        &mut self,
        by: EntityId,
        built: EntityId,
        at: DVec3,
    ) -> Result<(), String> {
        if !self.building.built.contains_key(&built) {
            return Err("it is not built: nothing to take apart".into());
        }
        if self.carried(by).is_some() {
            return Err("both hands are needed: put down what you hold".into());
        }
        self.joint_near(built, at)?;
        // (Ten seconds: a few hundred ticks.)
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let done = self.tick + (UNDOING * 60.0).ceil() as u64;
        let at = self.in_frame(built, at);
        let u = UnderWay {
            part: built,
            at,
            fastener: 0,
            done,
            undo: true,
        };
        self.building.under_way.insert(by, u);
        self.note(by, format!("taking apart: {UNDOING:.0} s"));
        Ok(())
    }

    /// The joint of structure `id` nearest `at`, if one is within reach of it.
    fn joint_near(&self, id: EntityId, at: DVec3) -> Result<usize, String> {
        let s = &self.world.bodies[self.index(id).ok_or("it is gone")?].structure;
        (0..s.element_count())
            .filter_map(|e| s.joint_ends(e).map(|(a, b)| (e, a, b)))
            .map(|(e, a, b)| (e, (0.5 * (s.position[a] + s.position[b]) - at).length()))
            .filter(|(_, d)| *d < NEAR)
            .min_by(|x, y| x.1.total_cmp(&y.1))
            .map(|(e, _)| e)
            .ok_or_else(|| "no joint there".into())
    }

    /// A joint taken apart, its time up: if its maker still reaches it.
    pub(super) fn unfasten_due(&mut self, by: EntityId, u: &UnderWay) -> Result<(), String> {
        let at = self.in_world(u.part, u.at).ok_or("it is gone")?;
        self.within_reach(by, at)?;
        let e = self.joint_near(u.part, at)?;
        let i = self.index(u.part).ok_or("it is gone")?;
        let body = &mut self.world.bodies[i];
        body.structure.sever(e);
        body.solver.refactor(&body.structure);
        body.wake();
        Ok(())
    }
}

impl ServerWorld {
    /// A blow of `energy` (J, with `tool`) by `by` on built structure `id` at `at`: it cuts into
    /// the member nearest there as wood is cut (`chop::chopped`), and once its notch reaches
    /// through the member's thickness (`chop::notch_volume`) the member is cut there in two.
    /// Returns the volume it cut (none: not a built structure), for the binding's load.
    pub(super) fn cut_member(
        &mut self,
        by: EntityId,
        id: EntityId,
        at: DVec3,
        (energy, tool): (f64, Option<u8>),
    ) -> Option<f64> {
        let record = self.building.built.get(&id)?;
        let i = self.index(id)?;
        let s = &self.world.bodies[i].structure;
        let (e, t, thick) = (0..s.element_count())
            .filter_map(|e| s.member(e).map(|(a, b, thick)| (e, a, b, thick)))
            .map(|(e, a, b, thick)| {
                let (pa, pb) = (s.position[a], s.position[b]);
                let d = pb - pa;
                let t = ((at - pa).dot(d) / d.length_squared()).clamp(0.05, 0.95);
                (e, t, thick, (pa + d * t - at).length())
            })
            .min_by(|x, y| x.3.total_cmp(&y.3))
            .map(|(e, t, thick, _)| (e, t, thick))?;
        let wood = record.woods.get(e).cloned().flatten()?;
        let across = (self.building.materials.as_ref())
            .and_then(|m| m.get(&wood))
            .and_then(|m| m.across)?;
        let volume = creeer_sim::trees::chop::chopped(energy, self.striking_edge(by, tool), across);
        if volume <= 0.0 {
            self.note(by, "it only bruised it: nothing sharp".into());
            return Some(0.0);
        }
        let through = creeer_sim::trees::chop::notch_volume(0.5 * thick, thick);
        let cut = self.building.cuts.entry((id, e)).or_insert(0.0);
        *cut += volume;
        let share = (*cut / through).min(1.0);
        if share < 1.0 {
            self.note(by, format!("a cut {:.0} % through", 100.0 * share));
            return Some(volume);
        }
        self.building.cuts.remove(&(id, e));
        let body = &mut self.world.bodies[i];
        body.structure.cut_at(e, t);
        // (Two nodes more: its solver made anew.)
        body.solver = creeer_physics::structure::ImplicitSolver::new(
            &body.structure,
            creeer_physics::structure::Scheme::Newmark,
            DT,
            1,
        );
        body.wake();
        if let Some(r) = self.building.built.get_mut(&id) {
            r.woods.push(Some(wood));
        }
        self.note(by, "cut through".into());
        Some(volume)
    }
}
