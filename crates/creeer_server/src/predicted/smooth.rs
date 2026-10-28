//! The drawn bodies slid after a correction (ADR-0008, ADR-0025): the avatar from where it was
//! drawn, each body from where it was drawn or interpolated.

use super::Predicted;
use creeer_math::glam::DVec3;
use creeer_net::entity::EntityId;
use std::collections::BTreeMap;

impl Predicted {
    /// The drawn bodies' offsets after a correction: the avatar's and each body's slide from
    /// where it was drawn; a body coming in slides from where it was interpolated, one going
    /// out slides from where it was predicted to where it is interpolated.
    pub(super) fn smooth(
        &mut self,
        before: Option<DVec3>,
        was: &BTreeMap<EntityId, DVec3>,
        drawn: impl Fn(EntityId) -> Option<DVec3>,
    ) {
        if let Some(before) = before {
            let after = self.world.bodies[0].structure.position[0];
            self.smoothing.corrected(before, after);
            // The tick before moves with the correction, so the drawn avatar stays where it was.
            if let Some(p) = &mut self.previous {
                p.position += after - before;
            }
            let c = &mut self.corrections;
            let by = (after - before).length();
            c.count += 1;
            c.over_a_centimetre += u32::from(by > 0.01);
            c.largest = c.largest.max(by);
        }
        let now: BTreeMap<EntityId, DVec3> = (self.nearby.iter())
            .zip(self.world.bodies.iter().skip(1))
            .map(|(id, b)| (*id, b.structure.position[0]))
            .collect();
        for (id, &after) in &now {
            let from = was.get(id).copied().or_else(|| drawn(*id));
            let smoothing = self.drawn.entry(*id).or_default();
            if let Some(before) = from {
                smoothing.corrected(before, after);
            }
        }
        for (id, &at) in was {
            if !now.contains_key(id) {
                let offset = self.drawn.remove(id).unwrap_or_default();
                let mut released = offset;
                if let Some(to) = drawn(*id) {
                    released.corrected(at, to);
                }
                self.released.insert(*id, released);
            }
        }
        self.drawn.retain(|id, _| now.contains_key(id));
    }
}
