//! The survival start (G1.6b, `docs/g1-plan.md` principle 9): a player has the clothes they wear
//! and nothing else. No jetpack (its tank, and so its gas, is not on the body) and no tools: what
//! they hold, they made. Dead, they start again at the spawn with nothing, and what they carried
//! lies where they fell; nothing is refilled. A creative server hands out the jetpack and the
//! tools, and refills the jetpack at the spawn.

use super::*;
use creeer_sim::items::Item;

impl ServerWorld {
    /// Avatar `id`, new: in survival, its body without the jetpack's tank.
    pub(super) fn equip(&mut self, id: EntityId) {
        if self.creative {
            return;
        }
        let load = self.items.load(id);
        if let Some(i) = self.index(id) {
            let body = &mut self.world.bodies[i];
            body.solver
                .set_mass(&mut body.structure, 0, character::DRY + load);
        }
    }

    /// Avatar `id`, started again at the spawn after dying at `lay`: what its pockets held left
    /// there; on a creative server, its jetpack refilled.
    pub(super) fn respawned(&mut self, id: EntityId, lay: DVec3) {
        let up = character::plumb(&self.world, lay);
        let (east, _) = creeer_terrain::geology::deposits::tangent_frame(up);
        let carried: Vec<Item> = self.change_pockets(id, |pockets| {
            (pockets.iter_mut())
                .flat_map(|p| std::mem::take(&mut p.items))
                .collect()
        });
        for (k, item) in carried.into_iter().enumerate() {
            #[allow(clippy::cast_precision_loss)] // a few
            let along = east * (0.15 * k as f64);
            self.add_item(item, lay + up * 0.3 + along, true);
        }
        if self.creative
            && let Some(i) = self.index(id)
        {
            character::refill(&mut self.world.bodies[i], 0.0);
        }
    }
}
