//! What the server tells a client that changes for good: the terrain's edits, structures'
//! lasting deformation.

use super::Connection;
use creeer_math::glam::DVec3;
use creeer_net::entity::{EntityId, Kind};
use creeer_net::message::Plastic;
use creeer_net::quant::POSITION_SCALE;
use creeer_terrain::edits::Edits;

impl Connection {
    /// Edits from the `first`-th on: those before it kept, these replacing the rest.
    pub(super) fn edited(&mut self, first: u64, log: &[u8]) {
        let Ok(part) = Edits::from_bytes(log) else {
            return;
        };
        let Ok(first) = usize::try_from(first) else {
            return;
        };
        if first > self.edits.len() {
            return; // never, in order: a gap would be a server's mistake
        }
        let replaced = first < self.edits.len();
        let mut edits = Edits::default();
        for edit in self.edits.log()[..first].iter().chain(part.log()) {
            edits.push(*edit);
        }
        self.edits = edits;
        if replaced {
            self.edits_generation += 1;
        }
        if let Some(terrain) = &self.terrain {
            if replaced {
                terrain.set_edits(self.edits.clone());
            } else {
                for edit in part.log() {
                    terrain.edit(*edit);
                }
            }
        }
    }

    pub(super) fn deformed(&mut self, id: EntityId, plastic: &Plastic) {
        let Some(Kind::Structure(frame)) = self.kinds.get_mut(&id) else {
            return;
        };
        for (node, at) in &plastic.nodes {
            let node = *node as usize;
            if node >= frame.nodes.len() {
                frame.nodes.resize(node + 1, DVec3::ZERO);
            }
            frame.nodes[node] = at.value(POSITION_SCALE);
        }
        if let Some(members) = &plastic.members {
            frame.members.clone_from(members);
            self.broken.remove(&id);
        }
        if !plastic.broken.is_empty() {
            self.broken.entry(id).or_default().extend(&plastic.broken);
        }
    }
}
