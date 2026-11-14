//! A nail driven into a part in the hands holds at its last blow (G2.11 B2, ADR-0036 amended
//! 2026-11-14, second pass). Before, the nails driven while a part was held were joints only once
//! it was let go: held on as a structure just made, the solver had failed (G2.10), and between
//! nails nothing held the part to what it was nailed to.
//!
//! Now the nail is a joint at its last blow, and the hands hold on to the structure the part has
//! become: at the node of it nearest where they held it, still where it is (the other hand
//! free for the next nail), with what they held before (their plan's place and turn moved
//! there, so nothing lurches). The structure now holds what it was nailed to, which the body may
//! stand on: the body meets it again at once (held things pass the body's box, and passing a
//! deck the body stood on it fell through).

use super::crossing::Against;
use super::*;
use creeer_building::fasteners::Fastener;

impl ServerWorld {
    /// Where carrier `by`'s hands hold their load now (world), if they hold one.
    fn held_point(&self, by: EntityId) -> Option<DVec3> {
        let c = self.carrying.get(&by)?;
        let s = &self.world.bodies[self.index(c.load)?].structure;
        Some(s.position[c.node] + s.orientation[c.node] * c.grip)
    }

    /// One of `fastener` driven by `by` where `part` touches `against` at `touch`, aimed at
    /// `at`, one of the two being what their hands hold: a joint at once, the parts one under
    /// the held part's id, and the hands hold on. (Aimed at the stringer under a board held, the
    /// board was taken into the stringer and the hands let go of it.)
    pub(in crate::world) fn drive_held(
        &mut self,
        by: EntityId,
        (part, against, touch): (EntityId, Against, DVec3),
        fastener: &Fastener,
        at: DVec3,
    ) -> Result<(), String> {
        let held = self.held_point(by).ok_or("nothing in the hands")?;
        let keep = self.carried(by).ok_or("nothing in the hands")?;
        let c = self.crossing(Some(by), part, against, touch)?;
        let driven = self.drive(keep, &c, fastener, at);
        // (Made a structure even if the joint was refused after: held on alike.)
        self.hold_on(by, held);
        driven
    }

    /// Carrier `by`'s hands hold on to their load, made a structure or grown: at its node
    /// nearest `held` (where they held it, world), still where it is, until let go.
    fn hold_on(&mut self, by: EntityId, held: DVec3) {
        let Some((i, b)) =
            (self.carrying.get(&by)).and_then(|c| Some((self.index(by)?, self.index(c.load)?)))
        else {
            return;
        };
        let centre = self.world.bodies[i].structure.position[0];
        let s = &self.world.bodies[b].structure;
        let Some(node) = (0..s.node_count())
            .filter(|&n| !s.held_fast(n))
            .min_by(|&m, &n| {
                (s.position[m] - held)
                    .length()
                    .total_cmp(&(s.position[n] - held).length())
            })
        else {
            return;
        };
        let (p, q) = (s.position[node], s.orientation[node]);
        if let Some(c) = self.carrying.get_mut(&by) {
            // At the node itself: aimed, the hands hold a thing at its node (its balance point),
            // and a grip off it was moved onto it, the hands then pulling the node to where the
            // point had been (a deck pole nailed at one end shifted off the other sill).
            c.node = node;
            c.grip = DVec3::ZERO;
            c.steady = Some((p, q));
            c.plan = super::super::carry::Plan {
                at: p - centre,
                velocity: DVec3::ZERO,
                turn: q,
                spin: DVec3::ZERO,
                there: true,
            };
        }
        self.building.pinned.insert(by);
        // What it was nailed to may be under the body's feet: met again at once.
        self.world.let_collide(i, b);
        self.changed.push(by);
        self.grip_taken(by);
    }
}
