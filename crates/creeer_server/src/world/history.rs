//! Where the bodies near players were (G1.4d, ADR-0027): so what a player reaches for is judged
//! where they saw it. A client draws other bodies a little in the past, between two snapshots
//! (`creeer_net::interp`), so a moving body it aims at has moved on by the time the server
//! hears of it; as Source's lag compensation does, the server looks the body up as it was at
//! the tick the client drew (`creeer_sim::reach::HISTORY` ticks back at most). Only bodies
//! near a player are kept: one cannot reach for the others.

use creeer_math::glam::{DQuat, DVec3};
use creeer_net::entity::EntityId;
use std::collections::VecDeque;

/// How near a player's avatar a body is kept (m): the reach from the hands (2.5 m), their
/// height on the body (under a metre), and the 3.5 m a running character covers in the
/// history's second, rounded up.
pub(super) const KEPT: f64 = 8.0;

/// Each node's centre and orientation.
pub(super) type Nodes = Vec<(DVec3, DQuat)>;

/// The bodies near players, tick by tick, the newest last.
#[derive(Debug, Default)]
pub(super) struct History {
    ticks: VecDeque<(u64, Vec<(EntityId, Nodes)>)>,
}

impl History {
    /// Keeps `bodies` (sorted by id) as they were at `tick`, forgetting the ticks more than
    /// `keep` before it.
    pub(super) fn record(&mut self, tick: u64, bodies: Vec<(EntityId, Nodes)>, keep: u64) {
        debug_assert!(bodies.is_sorted_by_key(|b| b.0));
        while (self.ticks.front()).is_some_and(|(t, _)| *t + keep < tick) {
            self.ticks.pop_front();
        }
        self.ticks.push_back((tick, bodies));
    }

    /// Entity `id`'s nodes at `tick`, if it was near a player then and the tick is kept.
    pub(super) fn at(&self, tick: u64, id: EntityId) -> Option<&Nodes> {
        let (oldest, _) = self.ticks.front()?;
        let k = usize::try_from(tick.checked_sub(*oldest)?).ok()?;
        let (t, bodies) = self.ticks.get(k)?;
        debug_assert_eq!(*t, tick);
        let i = bodies.binary_search_by_key(&id, |b| b.0).ok()?;
        Some(&bodies[i].1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_body_is_found_as_it_was_for_a_second_and_then_forgotten() {
        let mut h = History::default();
        let (a, b) = (EntityId(3), EntityId(7));
        for tick in 10..100 {
            #[allow(clippy::cast_precision_loss)]
            let x = tick as f64;
            let mut near = vec![(a, vec![(DVec3::X * x, DQuat::IDENTITY)])];
            // B near a player only from tick 50 on.
            if tick >= 50 {
                near.push((b, vec![(DVec3::Y * x, DQuat::IDENTITY)]));
            }
            h.record(tick, near, 60);
        }
        assert_eq!(h.at(99, a).unwrap()[0].0, DVec3::X * 99.0);
        assert_eq!(h.at(39, a).unwrap()[0].0, DVec3::X * 39.0);
        // Too old, not yet, or not near a player then.
        assert!(h.at(38, a).is_none());
        assert!(h.at(100, a).is_none());
        assert!(h.at(49, b).is_none());
        assert_eq!(h.at(50, b).unwrap()[0].0, DVec3::Y * 50.0);
    }
}
