//! The world to and from a save: its bodies, its entities, and its parked islands.

use super::regions::Tag;
use super::{DT, ServerWorld};
use creeer_data::Data;
use creeer_net::entity::EntityId;
use creeer_physics::world::World;
use creeer_sim::lod::Island;
use creeer_sim::planet_gravity;
use creeer_sim::save::{ParkedBody, Save};
use creeer_terrain::Terrain;
use std::sync::Arc;

impl ServerWorld {
    /// The world a save holds, on `terrain` (its planet's; the save's edits replace its own).
    pub fn from_save(data: Arc<Data>, save: Save, terrain: Arc<Terrain>) -> Self {
        let water = save.water.clone();
        terrain.set_edits(save.edits);
        let mut world = save.world;
        if world.planet.is_none() {
            // A version 1 save: no world was saved with it.
            world = World::new(DT);
            world.planet = Some(planet_gravity(terrain.planet()));
            world.static_friction = 0.7;
        }
        let mut this = Self::with(data, terrain, world, save.planet, save.tick);
        this.next_id = save.next_id;
        (this.ids, this.players) = save
            .entities
            .into_iter()
            .map(|(id, p)| (EntityId(id), p))
            .unzip();
        for (index, player) in this.players.iter().enumerate() {
            if player.is_some() {
                this.surroundings.spare(index);
            }
        }
        this.surroundings.restore(water);
        this.reindex();
        for island in save.parked {
            let bodies = (island.into_iter())
                .map(|p| {
                    let tag = Tag {
                        id: EntityId(p.id),
                        player: p.player,
                        water: p.water,
                    };
                    (p.body, tag)
                })
                .collect();
            this.parked.park(Island::new(bodies));
        }
        this
    }

    /// The world as a save.
    pub fn to_save(&self) -> Save {
        let mut parked: Vec<Vec<ParkedBody>> = (self.parked.iter())
            .map(|i| i.bodies.iter().map(parked_body).collect())
            .collect();
        // Bodies on their way back are saved as one more island.
        if !self.returning.is_empty() {
            parked.push(self.returning.iter().map(parked_body).collect());
        }
        Save {
            planet: self.planet.clone(),
            tick: self.tick,
            next_id: self.next_id,
            edits: self.terrain.edits().clone(),
            world: self.world.clone(),
            entities: self
                .ids
                .iter()
                .map(|id| id.0)
                .zip(self.players.iter().cloned())
                .collect(),
            water: self.surroundings.water(),
            parked,
        }
    }
}

fn parked_body((body, tag): &(creeer_physics::world::Body, Tag)) -> ParkedBody {
    ParkedBody {
        id: tag.id.0,
        player: tag.player.clone(),
        body: body.clone(),
        water: tag.water.clone(),
    }
}
