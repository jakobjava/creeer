//! Every entity's age (G1.3c, ADR-0026, `creeer_sim::ageing`): weathering at the rate its
//! surroundings set, and wear from the impacts its body takes. Kept by entity, so parking and
//! reordering bodies leave it alone (a parked island weathers on: its age follows from time),
//! and told to the sessions when it changes.

use super::ServerWorld;
use creeer_math::glam::DVec3;
use creeer_net::entity::EntityId;
use creeer_net::message::Age;
use creeer_sim::ageing::{self, Weathering};
use std::collections::{BTreeMap, HashMap, HashSet};

/// How often (ticks) awake bodies' surroundings are looked at: once a second.
const SURROUNDINGS_EVERY: u64 = 60;
/// How often (ticks) the ages of entities gone from the world are dropped: every ten seconds.
const PRUNE_EVERY: u64 = 600;

/// The world's ages.
#[derive(Clone, Debug)]
pub struct Ages {
    pub(super) by_entity: BTreeMap<EntityId, Weathering>,
    /// Awake bodies' velocities after the last step: an impact is a jolt from it.
    velocity: HashMap<EntityId, DVec3>,
    /// Entities whose age changed since the sessions were last told.
    changed: Vec<EntityId>,
    /// The world's pace: years of weathering per second at the reference rate.
    pub pace: f64,
}

impl Default for Ages {
    fn default() -> Self {
        Self {
            by_entity: BTreeMap::new(),
            velocity: HashMap::new(),
            changed: Vec::new(),
            pace: ageing::PACE,
        }
    }
}

/// What a step did to one body's age.
struct Event {
    id: EntityId,
    impact: f64,
    rate: Option<f64>,
}

impl ServerWorld {
    /// The rate a body at `p` weathers at: the sea's under it, the air's or vacuum's above.
    pub(super) fn weathering_rate(&self, p: DVec3) -> f64 {
        let depth = self.surroundings.sea.as_ref().map(|s| s.depth(p));
        ageing::rate(self.ages.pace, depth, self.surroundings.sky.is_some())
    }

    /// Entity `id`'s age, if it has one.
    pub fn age(&self, id: EntityId) -> Option<Weathering> {
        self.ages.by_entity.get(&id).copied()
    }

    /// Entity `id`'s age as a client is told it at network tick `tick`: its exposure now.
    pub fn age_told(&self, id: EntityId, tick: u32) -> Option<Age> {
        self.age(id).map(|w| Age {
            exposure: w.exposure_at(self.tick, super::DT),
            rate: w.rate,
            at: tick,
            wear: w.wear,
        })
    }

    /// The entities whose age changed since the last call, with it.
    pub fn take_aged(&mut self) -> Vec<(EntityId, Weathering)> {
        let changed = std::mem::take(&mut self.ages.changed);
        let mut unique: Vec<EntityId> = changed;
        unique.sort_unstable();
        unique.dedup();
        (unique.into_iter())
            .filter_map(|id| self.age(id).map(|w| (id, w)))
            .collect()
    }

    /// After a step: impacts wear awake bodies, and once a second their surroundings set
    /// their rate. A body met for the first time (new, or from a save without ages) starts new.
    pub(super) fn age_after_step(&mut self) {
        let look = self.tick.is_multiple_of(SURROUNDINGS_EVERY);
        let mut events = Vec::new();
        for (i, body) in self.world.bodies.iter().enumerate() {
            let id = self.ids[i];
            if body.sleep.asleep {
                self.ages.velocity.remove(&id);
                continue;
            }
            let v = body.structure.velocity[0];
            let impact = match self.ages.velocity.insert(id, v) {
                Some(before) => {
                    ageing::impact_energy(body.structure.total_mass(), before.into(), v.into())
                }
                None => 0.0,
            };
            let known = self.ages.by_entity.contains_key(&id);
            let rate = (look || !known).then(|| self.weathering_rate(body.structure.position[0]));
            if impact > 0.0 || rate.is_some() {
                events.push(Event { id, impact, rate });
            }
        }
        let tick = self.tick;
        for e in events {
            let age = self.ages.by_entity.entry(e.id).or_insert_with(|| {
                self.ages.changed.push(e.id);
                Weathering::new(tick, e.rate.unwrap_or(0.0))
            });
            let rated = e.rate.is_some_and(|r| age.set_rate(tick, super::DT, r));
            if age.strike(e.impact) || rated {
                self.ages.changed.push(e.id);
            }
        }
        if tick.is_multiple_of(PRUNE_EVERY) {
            self.prune_ages();
        }
    }

    /// Drops the ages of entities gone from the world (not those parked: they weather on).
    fn prune_ages(&mut self) {
        // Only the aged not in the world are looked for among the parked (a set of every parked
        // id, 100,000 of them in the stress scene, took 14 ms each time: G1.7).
        let mut missing: HashSet<EntityId> = (self.ages.by_entity.keys())
            .chain(self.ages.velocity.keys())
            .filter(|id| self.index(**id).is_none())
            .copied()
            .collect();
        if missing.is_empty() {
            return;
        }
        for island in self.parked.iter() {
            for (_, tag) in &island.bodies {
                missing.remove(&tag.id);
            }
        }
        self.ages.by_entity.retain(|id, _| !missing.contains(id));
        self.ages.velocity.retain(|id, _| !missing.contains(id));
    }
}
