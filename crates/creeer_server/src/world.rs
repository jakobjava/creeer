//! The authoritative world: the physics world on a planet's terrain, every body in it an
//! entity with an id that clients know it by, and what players may ask of it.
//!
//! The server checks what it is asked (plan.md §1, server-authoritative): a command acts
//! only within [`REACH`] of the asking player's avatar, an edit's brush is no larger than a
//! tool makes one, and nothing is placed at a point that is not finite.

use crate::things::{frame, upright};
use creeer_core::profile;
use creeer_data::Data;
use creeer_materials::species::SpeciesTable;
use creeer_math::glam::DVec3;
use creeer_net::channel::TICK_RATE;
use creeer_net::entity::EntityId;
use creeer_net::message::{AvatarInput, Command};
use creeer_physics::world::shape::Shape;
use creeer_physics::world::{Body, World};
use creeer_sim::surroundings::Surroundings;
use creeer_sim::{TerrainGround, character, planet_gravity};
use creeer_terrain::edits::{Brush, Edit, Edits};
use creeer_terrain::{Planet, Terrain};
use std::collections::HashMap;
use std::sync::Arc;

/// The time step (s).
pub const DT: f64 = 1.0 / TICK_RATE as f64;
/// How far from a player's avatar a command may act (m).
pub const REACH: f64 = 500.0;
/// The largest brush radius an edit may have (m).
pub const MOST_BRUSH: f64 = 10.0;
/// The longest tunnel an edit may bore (m).
pub const MOST_TUNNEL: f64 = 60.0;

/// A planet's terrain from its file `planet` in the packs `data` (`"planets/home.ron"`).
///
/// # Errors
/// If the file is not there or is not a valid planet.
pub fn load_terrain(data: &Data, planet: &str) -> Result<Arc<Terrain>, String> {
    let planet = Planet::load(data, planet).map_err(|e| e.to_string())?;
    Ok(Arc::new(Terrain::new(planet)))
}

/// The world.
#[derive(Debug)]
pub struct ServerWorld {
    /// The data packs the world is made of (clients must have the same).
    pub data: Arc<Data>,
    /// The terrain (its edits are the world's).
    pub terrain: Arc<Terrain>,
    /// The physics world.
    pub world: World,
    /// The planet's data file.
    pub planet: String,
    /// Ticks stepped.
    pub tick: u64,
    /// For each body of `world`: its id, and the player whose avatar it is.
    ids: Vec<EntityId>,
    players: Vec<Option<String>>,
    index: HashMap<EntityId, usize>,
    next_id: u32,
    /// Where avatars first appear: on land, on the ground.
    spawn: DVec3,
    /// The planet's air and sea on its bodies (F6.6), characters too (G1.2: their players'
    /// machines predict them in the same).
    surroundings: Surroundings,
    /// Resting islands far from every player (F7.2, `creeer_sim::lod`).
    parked: creeer_sim::lod::Parked<regions::Tag>,
    /// Parked bodies found near a player, on their way back (a few each tick).
    returning: std::collections::VecDeque<(Body, regions::Tag)>,
    /// Its machines (F7.4), and the next id for one.
    machines: Vec<machines::Placed>,
    next_machine: u32,
    /// The characters' legs' reactions on what they stand on, for this step alone
    /// (`creeer_sim::character::release`).
    reactions: Vec<character::Reaction>,
    /// The players' characters as people (G1.2d), by their avatars, and the entities that
    /// became something else since the sessions were last told (`people.rs`).
    people: std::collections::BTreeMap<EntityId, people::Person>,
    changed: Vec<EntityId>,
}

/// A point on the ground of `terrain`: on land between 300 and 1500 m (the first such of a
/// spread of directions, so always the same one).
fn spawn_point(terrain: &Terrain) -> DVec3 {
    let up = creeer_terrain::elevation::fibonacci_sphere(5000)
        .find(|&d| (300.0..1500.0).contains(&terrain.surface_height(d)))
        .unwrap_or(DVec3::Y);
    ground_under(
        terrain,
        up * (terrain.planet().radius + terrain.surface_height(up) + 50.0),
    )
}

/// The point of the ground of `terrain` under (or over) `p`, along the vertical.
fn ground_under(terrain: &Terrain, mut p: DVec3) -> DVec3 {
    let up = p.normalize();
    // The density is the height above the ground: down by it until on it.
    for _ in 0..30 {
        let d = terrain.density(p);
        p -= up * d;
        if d.abs() < 1e-4 {
            break;
        }
    }
    p
}

impl ServerWorld {
    /// A new world on `terrain` (the planet of data file `planet`), with nothing on it.
    pub fn new(data: Arc<Data>, terrain: Arc<Terrain>, planet: &str) -> Self {
        let mut world = World::new(DT);
        world.planet = Some(planet_gravity(terrain.planet()));
        world.static_friction = 0.7;
        Self::with(data, terrain, world, planet.to_owned(), 0)
    }

    fn with(
        data: Arc<Data>,
        terrain: Arc<Terrain>,
        mut world: World,
        planet: String,
        tick: u64,
    ) -> Self {
        world.ground = Some(Arc::new(TerrainGround(Arc::clone(&terrain))));
        let spawn = spawn_point(&terrain);
        let species = SpeciesTable::from_data(&data).expect("the packs' species load");
        let surroundings = Surroundings::of(terrain.planet(), &species)
            .expect("the planet's air is of known gases");
        Self {
            data,
            terrain,
            world,
            planet,
            tick,
            ids: Vec::new(),
            players: Vec::new(),
            index: HashMap::new(),
            next_id: 1,
            spawn,
            surroundings,
            parked: creeer_sim::lod::Parked::default(),
            returning: std::collections::VecDeque::new(),
            machines: Vec::new(),
            next_machine: 1,
            reactions: Vec::new(),
            people: std::collections::BTreeMap::new(),
            changed: Vec::new(),
        }
    }

    fn reindex(&mut self) {
        self.index = self
            .ids
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, i))
            .collect();
    }

    /// The body of entity `id`, by index.
    pub fn index(&self, id: EntityId) -> Option<usize> {
        self.index.get(&id).copied()
    }

    /// The entities, in the order of the world's bodies.
    pub fn ids(&self) -> &[EntityId] {
        &self.ids
    }

    /// The player whose avatar body `index` is.
    pub fn player(&self, index: usize) -> Option<&str> {
        self.players[index].as_deref()
    }

    /// The player whose character body `index` is drawn as (none for a fallen one's pelvis:
    /// it is drawn as the segment it is).
    pub fn drawn_as(&self, index: usize) -> Option<&str> {
        let player = self.players[index].as_deref()?;
        let fallen = (self.people.get(&self.ids[index])).is_some_and(people::Person::fallen);
        (!fallen).then_some(player)
    }

    /// The point of the ground under (or over) `p`, along the vertical.
    pub fn ground_under(&self, p: DVec3) -> DVec3 {
        ground_under(&self.terrain, p)
    }

    /// Every avatar.
    pub fn avatars(&self) -> Vec<EntityId> {
        self.ids
            .iter()
            .zip(&self.players)
            .filter(|(_, p)| p.is_some())
            .map(|(id, _)| *id)
            .collect()
    }
    /// Where avatars first appear.
    pub fn spawn(&self) -> DVec3 {
        self.spawn
    }

    /// Adds `body` (lifted clear of the ground), the avatar of `player` if any; returns its id.
    pub fn add_body(&mut self, body: Body, player: Option<String>) -> EntityId {
        self.add(body, player)
    }

    fn add(&mut self, body: Body, player: Option<String>) -> EntityId {
        let id = EntityId(self.next_id);
        self.next_id += 1;
        let index = self.world.add(body);
        self.lift_clear(index);
        self.index.insert(id, self.ids.len());
        self.ids.push(id);
        if player.is_some() {
            self.people.insert(id, people::Person::default());
        }
        self.players.push(player);
        id
    }

    /// Lifts body `index` along the vertical until no part of it is in the ground or in
    /// another body (one put on a slope, or where a crate stands, would be in it, and be
    /// thrown out of it), with a centimetre to spare.
    fn lift_clear(&mut self, index: usize) {
        let up = self.world.bodies[index].structure.position[0].normalize();
        for _ in 0..20 {
            let lift = self
                .world
                .touching(index, 0.0)
                .iter()
                .map(|t| (0.01 - t.gap) / t.normal.dot(up).max(0.2))
                .fold(0.0, f64::max);
            if lift <= 0.0 {
                break;
            }
            let body = &mut self.world.bodies[index];
            for p in &mut body.structure.position {
                *p += up * lift;
            }
            body.solver.reset_history(&body.structure);
        }
    }

    /// `player`'s avatar: the one they left, or a new one at the spawn point.
    pub fn avatar_of(&mut self, player: &str) -> EntityId {
        // Parked far away while its player was gone: back with its island.
        self.flush();
        if let Some(key) = self.parked.find(|t| t.player.as_deref() == Some(player)) {
            self.unpark(key);
        }
        if let Some(i) = self
            .players
            .iter()
            .position(|p| p.as_deref() == Some(player))
        {
            return self.ids[i];
        }
        let up = self.spawn.normalize();
        let (east, _) = creeer_terrain::geology::deposits::tangent_frame(up);
        // Side by side, two metres apart, a little above the ground.
        #[allow(clippy::cast_precision_loss)] // a few
        let along = self.players.iter().flatten().count() as f64 * 2.0;
        let feet = self.ground_under(self.spawn + east * along) + up * 0.05;
        self.add(character::body(feet, up, DT), Some(player.to_owned()))
    }

    /// Steers avatar `id` by `input` for the next step.
    pub fn steer(&mut self, id: EntityId, input: &AvatarInput) {
        // A fallen character is its ragdoll: there is nothing to steer.
        if self.people.get(&id).is_some_and(people::Person::fallen) {
            return;
        }
        if let Some(i) = self.index(id) {
            let at = self.world.bodies[i].structure.position[0];
            let water = self.surroundings.sea.as_ref().map(|sea| sea.depth(at));
            let act = character::Intent {
                walk: input.walk,
                jump: input.jump,
                thrust: input.thrust,
            };
            character::steer(
                &mut self.world,
                i,
                act,
                (at.normalize(), water),
                &mut self.reactions,
            );
        }
    }

    /// One step.
    pub fn step(&mut self) {
        let scope = profile::scope("machines");
        self.tick_machines();
        drop(scope);
        let scope = profile::scope("surroundings");
        self.surroundings.push(&mut self.world);
        drop(scope);
        self.people_before();
        self.world.step();
        character::release(&mut self.world, &mut self.reactions);
        let scope = profile::scope("people");
        self.people_after();
        drop(scope);
        let _scope = profile::scope("surroundings");
        self.surroundings.settle(&mut self.world, DT);
        self.tick += 1;
    }
}

/// The items of `v` whose bodies `map` kept.
fn kept<T: Clone>(map: &[Option<usize>], v: &[T]) -> Vec<T> {
    map.iter()
        .zip(v)
        .filter(|(m, _)| m.is_some())
        .map(|(_, x)| x.clone())
        .collect()
}

#[cfg(test)]
mod lod_tests;
#[cfg(test)]
mod machine_tests;
pub mod machines;
pub mod people;
mod persist;
mod regions;
pub use regions::Tag;
mod commands;
#[cfg(test)]
mod people_tests;
#[cfg(test)]
pub(crate) mod tests;
