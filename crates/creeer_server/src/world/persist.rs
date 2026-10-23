//! The world to and from a save: its bodies, its entities, its parked islands, its items.

use super::regions::Tag;
use super::{DT, ServerWorld};
use creeer_data::Data;
use creeer_net::entity::EntityId;
use creeer_physics::world::World;
use creeer_sim::lod::Island;
use creeer_sim::planet_gravity;
use creeer_sim::powerhouse::Powerhouse;
use creeer_sim::save::{ParkedBody, Save, SavedMachine};
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
        this.surroundings.restore(water);
        this.reindex();
        this.restore_people(&save.people);
        this.let_go_after_load();
        this.ages.by_entity = (save.ages.iter())
            .map(|(id, age)| (EntityId(*id), *age))
            .collect();
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
        this.items.bodies = (save.items.bodies.into_iter())
            .map(|(id, item)| (EntityId(id), item))
            .collect();
        this.items.pockets = (save.items.pockets.into_iter())
            .map(|(id, pockets)| (EntityId(id), pockets))
            .collect();
        this.items.taken_set = save.items.taken.iter().copied().collect();
        this.items.taken = save.items.taken;
        this.items.next_laid = (save.items.laid.iter())
            .map(|l| (l.0 & !(1 << 63)) + 1)
            .max()
            .unwrap_or(0);
        for (id, item, at, turn) in save.items.laid {
            this.items.laid.insert(id, (item, at, turn));
            this.items.laid_log.push(id);
        }
        this.next_machine = save.machines.iter().map(|m| m.id + 1).max().unwrap_or(1);
        this.machines = (save.machines.iter())
            .map(|m| super::machines::Placed::restored(m, Powerhouse::new()))
            .collect();
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
        // Machines catching up are not kept (`Server::save` takes them in first).
        let machines = (self.machines.iter())
            .filter_map(|m| {
                m.house().map(|h| SavedMachine {
                    id: m.id,
                    kind: "powerhouse".into(),
                    at: m.at,
                    born: m.born,
                    snapshot: h.snapshot(),
                })
            })
            .collect();
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
            machines,
            people: self.saved_people(),
            ages: (self.ages.by_entity.iter())
                .map(|(id, age)| (id.0, *age))
                .collect(),
            items: creeer_sim::save::SavedItems {
                bodies: (self.items.bodies.iter())
                    .map(|(id, item)| (id.0, item.clone()))
                    .collect(),
                pockets: (self.items.pockets.iter())
                    .map(|(id, p)| (id.0, p.clone()))
                    .collect(),
                taken: self.items.taken.clone(),
                laid: (self.items.laid.iter())
                    .map(|(id, (item, at, turn))| (*id, item.clone(), *at, *turn))
                    .collect(),
            },
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
