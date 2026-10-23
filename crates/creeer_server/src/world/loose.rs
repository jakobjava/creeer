//! Picking up what lies loose (G1.6c, ADR-0029): a stone or a branch the scatter put on the
//! ground (`creeer_sim::scatter`) is scenery until a player picks it up; then it is an item,
//! in their hands or a pocket, and the world remembers it taken (saved, and told to every
//! client so none draws it again).

use super::*;
use creeer_sim::reach;
use creeer_sim::scatter::Loose;

impl ServerWorld {
    /// The colour item body `id` is drawn in: its material's.
    pub fn item_colour(&self, id: EntityId) -> Option<[u8; 3]> {
        let head = self.items.bodies.get(&id)?.head()?;
        self.items.densities.as_ref()?.colour(&head.material)
    }

    /// What was picked up, in order: every client has the first `told` already.
    pub fn taken_since(&self, told: usize) -> &[u64] {
        self.items.taken.get(told..).unwrap_or_default()
    }

    /// Avatar `by` picks up loose thing `id`: into its hands, or into its first pocket that it
    /// fits (`pocket`).
    pub(super) fn pick(&mut self, by: EntityId, id: u64, pocket: bool) -> Result<(), String> {
        let loose = self.loose_in_reach(by, id)?;
        let scatter = self.items.loose.as_ref().ok_or("nothing lies loose here")?;
        let densities = self.items.densities.as_ref().ok_or("no materials")?;
        let item = (scatter.item(&loose, &self.items.rocks, densities)).ok_or("unknown matter")?;
        if pocket {
            let pockets = self.items.pockets.get(&by).unwrap_or(&self.items.outfit);
            let k = (pockets.iter().position(|p| p.refuses(&item).is_none()))
                .ok_or("fits no pocket")?;
            self.take_loose(id);
            return self
                .change_pockets(by, |p| p[k].put(item))
                .map_err(|refused| refused.1);
        }
        self.hands_free(by)?;
        self.take_loose(id);
        let body = self.add_item(item, loose.centre, false);
        let i = self.index(body).expect("just added");
        self.world.bodies[i].structure.orientation[0] = loose.turn;
        let b = &mut self.world.bodies[i];
        b.solver.reset_history(&b.structure);
        let up = loose.centre.normalize();
        let top = (b.shapes.iter())
            .map(|(_, s)| s.extent(b.structure.orientation[0], up))
            .fold(0.0, f64::max);
        self.take_hold(by, (i, 0), loose.centre + up * top)
    }

    /// Avatar `by` strikes loose thing `id` with what it holds (its swing begun at the world's
    /// tick `started`): it is made an item where it lies, and the swing aimed at its top.
    pub(super) fn strike_loose(
        &mut self,
        by: EntityId,
        id: u64,
        started: f64,
    ) -> Result<(), String> {
        let ready = self.ready_to_swing(by, started)?;
        let loose = self.loose_in_reach(by, id)?;
        let scatter = self.items.loose.as_ref().ok_or("nothing lies loose here")?;
        let densities = self.items.densities.as_ref().ok_or("no materials")?;
        let item = (scatter.item(&loose, &self.items.rocks, densities)).ok_or("unknown matter")?;
        self.take_loose(id);
        let body = self.add_item(item, loose.centre, false);
        let i = self.index(body).expect("just added");
        let b = &mut self.world.bodies[i];
        b.structure.orientation[0] = loose.turn;
        b.solver.reset_history(&b.structure);
        let up = loose.centre.normalize();
        let top = (b.shapes.iter())
            .map(|(_, s)| s.extent(loose.turn, up))
            .fold(0.0, f64::max);
        let local = loose.turn.conjugate() * (up * top);
        self.begin_swing(by, ready, super::strikes::Aim::Body(body, 0, local));
        Ok(())
    }

    /// Loose thing `id`, if it lies there untaken within reach of avatar `by`'s hands.
    fn loose_in_reach(&self, by: EntityId, id: u64) -> Result<Loose, String> {
        if self.items.taken_set.contains(&id) {
            return Err("taken".into());
        }
        let scatter = self.items.loose.as_ref().ok_or("nothing lies loose here")?;
        let loose = scatter
            .find(&self.terrain, id)
            .ok_or("nothing lies there")?;
        let i = self.index(by).ok_or("no such avatar")?;
        let centre = self.world.bodies[i].structure.position[0];
        let hands = reach::hands(centre, character::plumb(&self.world, centre));
        let far = (loose.centre - hands).length();
        if far > reach::REACH + 0.5 {
            return Err(format!("{far:.1} m away: out of reach"));
        }
        Ok(loose)
    }

    fn take_loose(&mut self, id: u64) {
        if self.items.taken_set.insert(id) {
            self.items.taken.push(id);
        }
    }
}
