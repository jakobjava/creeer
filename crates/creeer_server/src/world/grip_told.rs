//! What clients are told of where the hands hold their load (G2.10, ADR-0036), so they draw
//! the hands there: the point held, in the frame the load is drawn in (its node 0's), and
//! whether in one hand or two (one while the other fastens: `hands.rs`). Told when it is taken;
//! again at once when the hands change, and when the point has moved more than [`MOVED`] (hand
//! over hand along a pole, at 0.5 m/s), at most every [`EVERY`] ticks, as a structure's shape is
//! told (`replicate.rs`). The client slides the drawn hands between.

use super::*;
use creeer_net::entity::Carried;

/// How far the point held moves before clients are told again (m), and how often at most
/// (ticks): a structure's shape's (`replicate.rs`).
const MOVED: f64 = crate::replicate::SHAPE_TOLERANCE;
const EVERY: u64 = crate::replicate::SHAPE_EVERY;

/// What clients were last told of a carrier's grip, and the tick it was told.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Told {
    pub(super) carried: Carried,
    pub(super) tick: u64,
}

impl ServerWorld {
    /// Carrier `by`'s grip as it is now: the point held in its load's node 0's frame, and one
    /// hand or two.
    fn grip_now(&self, by: EntityId) -> Option<Carried> {
        let c = self.carrying.get(&by)?;
        let s = &self.world.bodies[self.index(c.load)?].structure;
        let held = s.position[c.node] + s.orientation[c.node] * c.grip;
        let grip = s.orientation[0].conjugate() * (held - s.position[0]);
        let one_hand = c.one_hand || self.building.under_way.contains_key(&by);
        Some(Carried::new(c.load, grip, one_hand))
    }

    /// What clients are told avatar `by`'s hands hold, if anything.
    pub(super) fn told_grip(&self, by: EntityId) -> Option<Carried> {
        let c = self.carrying.get(&by)?;
        c.told.map(|t| t.carried).or_else(|| self.grip_now(by))
    }

    /// Avatar `by` took its load: its grip as told from now.
    pub(super) fn grip_taken(&mut self, by: EntityId) {
        let (now, tick) = (self.grip_now(by), self.tick);
        if let Some(c) = self.carrying.get_mut(&by) {
            c.told = now.map(|carried| Told { carried, tick });
        }
    }

    /// After the step: each carrier told anew whose hands changed (one while the other
    /// fastens, two again), or whose grip moved on (rate-limited).
    pub(super) fn tell_grips(&mut self) {
        let carriers: Vec<EntityId> = self.carrying.keys().copied().collect();
        for by in carriers {
            let Some(now) = self.grip_now(by) else {
                continue;
            };
            let tick = self.tick;
            let c = self.carrying.get_mut(&by).expect("a carrier");
            let due = c.told.is_none_or(|t| {
                t.carried.one_hand != now.one_hand
                    || ((t.carried.grip - now.grip).length() > MOVED && tick >= t.tick + EVERY)
            });
            if due {
                c.told = Some(Told { carried: now, tick });
                self.changed.push(by);
            }
        }
    }
}
