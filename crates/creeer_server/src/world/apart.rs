//! Built structures falling apart in the world (G2.5, ADR-0033): after each step, a structure
//! whose broken joints or members leave pieces (`World::fall_apart`) is one entity per piece.
//! The first keeps its entity; the others are new, each with its share of the structure's
//! record (its elements' woods; the items it was made of stay with the first, for G2.6).

use super::building::Built;
use super::*;

impl ServerWorld {
    /// Gives each piece of the structures that came apart in the last step its entity.
    pub(super) fn come_apart(&mut self) {
        for (i, pieces) in self.world.fall_apart() {
            let Some(&first) = self.ids.get(i) else {
                continue;
            };
            self.surroundings.forget(i);
            let record = self.building.built.remove(&first);
            for (k, (at, kept)) in pieces.into_iter().enumerate() {
                let id = if k == 0 {
                    first
                } else {
                    let id = EntityId(self.next_id);
                    self.next_id += 1;
                    self.index.insert(id, self.ids.len());
                    self.ids.push(id);
                    self.players.push(None);
                    debug_assert_eq!(self.ids.len() - 1, at, "pieces come at the world's end");
                    id
                };
                if let Some(r) = &record {
                    let woods = kept
                        .iter()
                        .map(|&e| r.woods.get(e).cloned().flatten())
                        .collect();
                    let items = if k == 0 { r.items.clone() } else { Vec::new() };
                    self.building.built.insert(id, Built { items, woods });
                }
            }
        }
    }
}
