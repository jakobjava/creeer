//! Picking up what lies on the ground (G1.6c, ADR-0029): a stone or a branch the scatter put
//! there (`creeer_sim::scatter`), or an item laid at rest (`laid.rs`), is scenery until a player
//! picks it up or strikes it; then it is an item, in their hands, a pocket or a body where it
//! lay, and the world remembers it taken (saved, and told to every client so none draws it
//! again).

use super::*;
use creeer_math::glam::DQuat;
use creeer_sim::items::Item;
use creeer_sim::reach;

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

    /// Avatar `by` picks up what lies at `id`: into its hands, or into its first pocket that it
    /// fits (`pocket`).
    pub(super) fn pick(&mut self, by: EntityId, id: u64, pocket: bool) -> Result<(), String> {
        let (item, centre, turn) = self.lying_in_reach(by, id)?;
        if pocket {
            let pockets = self.items.pockets.get(&by).unwrap_or(&self.items.outfit);
            let k = (pockets.iter().position(|p| p.refuses(&item).is_none()))
                .ok_or_else(|| format!("fits no pocket ({})", too_large(&item)))?;
            self.take_lying(id);
            return self
                .change_pockets(by, |p| p[k].put(item))
                .map_err(|refused| refused.1);
        }
        self.hands_free(by)?;
        self.take_lying(id);
        let (i, top) = self.rouse(item, centre, turn);
        self.take_hold(by, (i, 0), centre + centre.normalize() * top)
    }

    /// Avatar `by` strikes what lies at `id` with what it holds (its swing begun at the world's
    /// tick `started`): it is made a body where it lies, and the swing aimed at its top.
    pub(super) fn strike_loose(
        &mut self,
        by: EntityId,
        id: u64,
        started: f64,
    ) -> Result<(), String> {
        let ready = self.ready_to_swing(by, started)?;
        let (item, centre, turn) = self.lying_in_reach(by, id)?;
        self.take_lying(id);
        let (i, top) = self.rouse(item, centre, turn);
        let local = turn.conjugate() * (centre.normalize() * top);
        let body = self.ids()[i];
        self.begin_swing(by, ready, super::strikes::Aim::Body(body, 0, local));
        Ok(())
    }

    /// `item` made a body where it lay (`centre`, turned `turn`), at rest: its index, and how far
    /// its top is above its centre.
    fn rouse(&mut self, item: Item, centre: DVec3, turn: DQuat) -> (usize, f64) {
        let body = self.add_item(item, centre, false);
        let i = self.index(body).expect("just added");
        let b = &mut self.world.bodies[i];
        b.structure.orientation[0] = turn;
        b.solver.reset_history(&b.structure);
        let up = centre.normalize();
        let top = (b.shapes.iter())
            .map(|(_, s)| s.extent(turn, up))
            .fold(0.0, f64::max);
        (i, top)
    }

    /// What lies at `id`, if untaken within reach of avatar `by`'s hands: an item laid there, or
    /// the scatter's stone or branch as an item; with where it lies and its turn.
    fn lying_in_reach(&self, by: EntityId, id: u64) -> Result<(Item, DVec3, DQuat), String> {
        let lying = if let Some((item, centre, turn)) = self.items.laid.get(&id) {
            (item.clone(), *centre, *turn)
        } else {
            if self.items.taken_set.contains(&id) {
                return Err("taken".into());
            }
            let scatter = self.items.loose.as_ref().ok_or("nothing lies loose here")?;
            let loose = (scatter.find(&self.terrain, id)).ok_or("nothing lies there")?;
            let densities = self.items.densities.as_ref().ok_or("no materials")?;
            let item =
                (scatter.item(&loose, &self.items.rocks, densities)).ok_or("unknown matter")?;
            (item, loose.centre, loose.turn)
        };
        let i = self.index(by).ok_or("no such avatar")?;
        let centre = self.world.bodies[i].structure.position[0];
        let hands = reach::hands(centre, character::plumb(&self.world, centre));
        let far = (lying.1 - hands).length();
        if far > reach::REACH + 0.5 {
            return Err(format!("{far:.1} m away: out of reach"));
        }
        Ok(lying)
    }

    /// What lies at `id` taken from the ground: clients stop drawing it.
    fn take_lying(&mut self, id: u64) {
        self.items.laid.remove(&id);
        if self.items.taken_set.insert(id) {
            self.items.taken.push(id);
        }
    }
}

/// How large `item` is, in a few words.
fn too_large(item: &Item) -> String {
    let e = item.extents().unwrap_or_default();
    format!(
        "{:.0} × {:.0} × {:.0} cm",
        e[0] * 100.0,
        e[1] * 100.0,
        e[2] * 100.0
    )
}
