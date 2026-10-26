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
    /// `in_place`: these replace those edits only, the rest kept (a dig spot's brush grown).
    pub(super) fn edited(&mut self, first: u64, log: &[u8], in_place: bool) {
        let Ok(part) = Edits::from_bytes(log) else {
            return;
        };
        let Ok(first) = usize::try_from(first) else {
            return;
        };
        let old = self.edits.len();
        if first > old {
            return; // never, in order: a gap would be a server's mistake
        }
        // In place: as though the rest were sent again unchanged after them.
        let end = if in_place {
            (first + part.len()).max(old)
        } else {
            first + part.len()
        };
        // What changed, each version (a dig spot's brush grown is sent again, G2.9).
        let mut changed = Vec::new();
        for (i, edit) in (first..).zip(part.log()) {
            match self.edits.log().get(i) {
                Some(was) if was == edit => {}
                Some(was) => {
                    self.edits_dirty.extend([*was, *edit]);
                    changed.push(i);
                }
                None => self.edits_dirty.push(*edit),
            }
        }
        self.edits_dirty
            .extend(self.edits.log().get(end..).unwrap_or_default());
        if end >= old {
            // Changed in their places, the rest added: the index mended, not built anew.
            for &i in &changed {
                self.edits.replace(i, part.log()[i - first]);
            }
            for edit in part.log().get(old - first..).unwrap_or_default() {
                self.edits.push(*edit);
            }
            if let Some(terrain) = &self.terrain {
                for &i in &changed {
                    terrain.replace_edit(i, part.log()[i - first]);
                }
                for edit in part.log().get(old - first..).unwrap_or_default() {
                    terrain.edit(*edit);
                }
            }
        } else {
            let mut edits = Edits::default();
            for edit in self.edits.log()[..first].iter().chain(part.log()) {
                edits.push(*edit);
            }
            self.edits = edits;
            if let Some(terrain) = &self.terrain {
                terrain.set_edits(self.edits.clone());
            }
        }
        if !changed.is_empty() || end < old {
            self.edits_generation += 1;
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
        frame.nodes.truncate(plastic.node_count as usize);
        if let Some(radii) = &plastic.radii {
            frame.radii.clone_from(radii);
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
