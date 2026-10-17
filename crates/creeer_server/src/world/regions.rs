//! The world's simulation LOD (F7.2, ADR-0019): resting islands far from every player are
//! parked, and put back when a player comes near (`creeer_sim::lod`).

use super::{ServerWorld, kept};
use creeer_math::glam::DVec3;
use creeer_net::entity::EntityId;
use creeer_sim::lod::{self, Island};
use std::collections::BTreeMap;

/// What the world keeps of a parked body besides the body: its entity, whose avatar it is,
/// and the water moving with its floating shapes.
#[derive(Clone, Debug, PartialEq)]
pub struct Tag {
    pub id: EntityId,
    pub player: Option<String>,
    pub water: Vec<f64>,
}

impl ServerWorld {
    /// Simulation LOD, before each step; `centres` are the players' avatars (with none,
    /// every resting island is parked). Every [`lod::PERIOD`] ticks the parked islands within
    /// [`lod::ACTIVE`] of a centre are found (nearest first) and resting islands beyond
    /// [`lod::PARK`] parked; every tick up to [`lod::WAKE_PER_TICK`] bodies of the islands
    /// found are put back.
    pub fn regions(&mut self, centres: &[DVec3]) {
        if self.tick.is_multiple_of(lod::PERIOD) {
            for key in self.parked.near(centres, lod::ACTIVE) {
                if let Some(island) = self.parked.take(key) {
                    self.returning.extend(island.bodies);
                }
            }
            self.park(centres);
        }
        self.wake();
    }

    /// Puts back up to [`lod::WAKE_PER_TICK`] of the bodies returning, nearest island first.
    /// An island may come back over several ticks: its bodies are asleep, and nothing awake
    /// is within reach of them (a player is [`lod::ACTIVE`] away) before all are back.
    fn wake(&mut self) {
        let n = self.returning.len().min(lod::WAKE_PER_TICK);
        let back: Vec<_> = self.returning.drain(..n).collect();
        for (body, tag) in back {
            self.put_back(body, tag);
        }
    }

    /// Puts back every body still returning.
    pub(crate) fn flush(&mut self) {
        let back: Vec<_> = self.returning.drain(..).collect();
        for (body, tag) in back {
            self.put_back(body, tag);
        }
    }

    /// Puts back parked island `key`, its bodies as they were.
    pub(crate) fn unpark(&mut self, key: u64) {
        let Some(island) = self.parked.take(key) else {
            return;
        };
        for (body, tag) in island.bodies {
            self.put_back(body, tag);
        }
    }

    /// Puts a parked body back into the world.
    fn put_back(&mut self, body: creeer_physics::world::Body, tag: Tag) {
        let index = self.world.put_back(body);
        if tag.player.is_some() {
            self.surroundings.spare(index);
        }
        self.surroundings.expect(index, tag.water);
        self.index.insert(tag.id, self.ids.len());
        self.ids.push(tag.id);
        self.players.push(tag.player);
    }

    /// Takes out every resting island farther than [`lod::PARK`] from every centre.
    fn park(&mut self, centres: &[DVec3]) {
        let islands = lod::to_park(&self.world, centres);
        if islands.is_empty() {
            return;
        }
        let mut island_of = BTreeMap::new();
        for (k, members) in islands.iter().enumerate() {
            for &i in members {
                island_of.insert(i, k);
            }
        }
        let tags: Vec<(usize, Tag)> = (island_of.keys())
            .map(|&i| {
                let tag = Tag {
                    id: self.ids[i],
                    player: self.players[i].clone(),
                    water: self.surroundings.water_of(i),
                };
                (i, tag)
            })
            .collect();
        let (bodies, map) = self.world.take_bodies(|i, _| island_of.contains_key(&i));
        self.surroundings.retain(&map);
        (self.ids, self.players) = (kept(&map, &self.ids), kept(&map, &self.players));
        self.reindex();
        let mut grouped: Vec<Vec<_>> = (0..islands.len()).map(|_| Vec::new()).collect();
        for (body, (i, tag)) in bodies.into_iter().zip(tags) {
            grouped[island_of[&i]].push((body, tag));
        }
        for bodies in grouped {
            self.parked.park(Island::new(bodies));
        }
    }

    /// How many islands and bodies are parked (bodies on their way back count as one more
    /// island).
    pub fn parked(&self) -> (usize, usize) {
        let (islands, bodies) = self.parked.counts();
        let back = self.returning.len();
        (islands + usize::from(back > 0), bodies + back)
    }

    /// Whether entity `id` is parked (or on its way back).
    pub fn is_parked(&self, id: EntityId) -> bool {
        self.parked.find(|t| t.id == id).is_some() || self.returning.iter().any(|(_, t)| t.id == id)
    }
}
