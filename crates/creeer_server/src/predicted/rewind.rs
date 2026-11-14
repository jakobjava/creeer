//! Setting the prediction back to a snapshot (ADR-0025, ADR-0043): nothing where it agrees;
//! moved in place where it is off by little (`nudge.rs`); else rebuilt from the snapshot and the
//! inputs since replayed.

use super::Predicted;
use super::nearby;
use crate::world::DT;
use creeer_math::glam::DVec3;
use creeer_net::entity::{EntityId, Kind};
use creeer_net::predict::reconcile;
use creeer_net::quant::BodyState;
use creeer_net::snapshot::Snapshot;
use creeer_physics::world::World;
use creeer_sim::character;
use std::collections::BTreeMap;

impl Predicted {
    /// The server had the world as `snapshot` says (the avatar `avatar`, the entities
    /// `kinds`), after its input `snapshot.last_input`: the avatar and the bodies near it are
    /// set to it and the inputs since are applied again (if the snapshot is newer than the
    /// last one it was set to). `drawn` gives where an entity not predicted is drawn now
    /// (interpolated): a body coming into the prediction slides from there.
    pub fn reconcile(
        &mut self,
        snapshot: &Snapshot,
        (avatar, kinds): (EntityId, &BTreeMap<EntityId, Kind>),
        drawn: impl Fn(EntityId) -> Option<DVec3>,
    ) {
        let started = std::time::Instant::now();
        let tick = snapshot.tick;
        if self.fallen || self.reconciled.is_some_and(|t| t >= tick) {
            return;
        }
        let Some(state) = snapshot.states.get(&avatar) else {
            return;
        };
        self.reconciled = Some(tick);
        // The bodies it could touch before the next correction (ADR-0025, amended G2.10).
        let replayed = (self.log.pending())
            .filter(|i| i.seq > snapshot.last_input)
            .count();
        let reach = nearby::Reach::of(state, replayed);
        let picked = nearby::chosen(snapshot, kinds, &reach, &self.nearby);
        // Predicted as the server had it, and the same bodies near: nothing to correct.
        let chosen: Vec<EntityId> = picked.iter().map(|c| c.0).collect();
        let (avatar_agrees, bodies_off) = self.history.disagreeing(snapshot, avatar, &self.nearby);
        let last_input = snapshot.last_input;
        if chosen == self.nearby && avatar_agrees && bodies_off == 0 {
            self.history.forget_before(last_input);
            self.log.applied(last_input);
            self.corrections.count += 1;
            self.corrections.replaying += started.elapsed();
            return;
        }
        let before = self.world.bodies.first().map(|b| b.structure.position[0]);
        let was: BTreeMap<EntityId, DVec3> = (self.nearby.iter())
            .zip(self.world.bodies.iter().skip(1))
            .map(|(id, b)| (*id, b.structure.position[0]))
            .collect();
        // Off by little: moved in place, nothing replayed (`nudge.rs`).
        if chosen == self.nearby && self.nudge(snapshot, avatar) {
            self.history.forget_before(last_input);
            self.log.applied(last_input);
            self.smooth(before, &was, drawn);
            self.corrections.replaying += started.elapsed();
            return;
        }
        self.count_rewind(snapshot, avatar, (avatar_agrees, bodies_off));
        let mut world = self.afresh(state);
        // The bodies near it, as the server had them at this input (ADR-0025).
        self.templates.retain(kinds);
        for chosen in picked {
            let stale = snapshot.stale.binary_search(&chosen.0).is_ok();
            let from = (stale, &self.history, last_input);
            if self.templates.add(&mut world, chosen, from) {
                self.nearby.push(chosen.0);
            }
        }
        self.history.forget_before(last_input);
        let around = self.around.take();
        let (ids, history, load) = (&self.nearby, &mut self.history, self.load);
        let c = &mut self.corrections;
        (self.world, self.around) = reconcile(
            (world, around),
            last_input,
            &mut self.log,
            |(w, a), input| {
                let worked = Self::step(w, a, input, load);
                (c.work, c.replay_work) = (c.work + worked, c.replay_work + worked);
                c.replayed += 1;
                history.record(input.seq, ids, w);
            },
        );
        self.smooth(before, &was, drawn);
        self.corrections.replaying += started.elapsed();
    }

    /// Counts a rewind: why (the avatar off, a body off, or the bodies chosen changed) and by
    /// how much (`Corrections`).
    fn count_rewind(&mut self, snapshot: &Snapshot, avatar: EntityId, (me, bodies): (bool, usize)) {
        let c = &mut self.corrections;
        c.rewinds += 1;
        if !me {
            c.for_avatar += 1;
        } else if bodies > 0 {
            c.for_bodies += 1;
        } else {
            c.for_choice += 1;
        }
        if let Some(((me, _), (them, _), which)) = self.history.off(snapshot, avatar) {
            c.count_off(me, them);
            for (counts, off) in c.off_in.iter_mut().zip(which) {
                for (n, o) in counts.iter_mut().zip(off) {
                    *n += u32::from(o);
                }
            }
        }
    }

    /// A prediction world built afresh: the avatar alone, as the server has it (`state`).
    fn afresh(&mut self, state: &BodyState) -> World {
        let mut world = std::mem::replace(&mut self.world, World::new(DT));
        // (The air and sea know the bodies by index: told what was kept.)
        let kept = world.retain_bodies(|i, _| i == 0);
        if let Some(a) = self.around.as_mut() {
            a.retain(&kept);
        }
        self.nearby.clear();
        if world.bodies.is_empty() {
            world.add_copy(character::body(
                state.position(),
                state.position().normalize(),
                DT,
            ));
        }
        let body = &mut world.bodies[0];
        body.wake();
        let s = &mut body.structure;
        s.position[0] = state.position();
        s.orientation[0] = state.orientation();
        s.velocity[0] = state.velocity();
        s.angular_velocity[0] = state.spin();
        body.solver.set_mass(
            &mut body.structure,
            0,
            character::DRY + self.gas + self.load,
        );
        body.solver.reset_history(&body.structure);
        // Asleep on the server, asleep here too, as the bodies near it are (`nearby.rs`): a
        // character standing still sleeps, and woken it would be stepped where the server's
        // stands as it fell asleep (G1.3i: standing still no longer creeps, so it does sleep).
        if state.asleep {
            body.sleep_now();
        }
        world
    }
}
