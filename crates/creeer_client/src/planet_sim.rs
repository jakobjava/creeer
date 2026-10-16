//! F3 planet sandbox simulation: physics on the reference planet's terrain, in its rotating
//! frame, with terrain edits applied in the simulation tick (server-authoritative, plan.md §1).

use crate::physics_sandbox::PhysicsSnapshot;
use creeer_core::Simulation;
use creeer_math::glam::{DQuat, DVec3};
use creeer_physics::world::shape::Shape;
use creeer_physics::world::{Body, World};
use creeer_sim::{TerrainGround, planet_gravity};
use creeer_terrain::Terrain;
use creeer_terrain::edits::{Edit, Edits};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

/// What the UI asks the simulation to do (applied at the start of the next tick).
#[derive(Clone, Debug)]
pub enum Command {
    /// Drop a 1 m crate (200 kg) at a position, at rest.
    Crate(DVec3),
    /// Drop a ball (radius 0.4 m, 300 kg) at a position, at rest.
    Ball(DVec3),
    /// Change the terrain.
    Edit(Edit),
    /// Replace all edits (loading a save).
    LoadEdits(Edits),
    /// Remove every body.
    Clear,
}

/// The UI's handle for sending commands.
pub type Commands = Arc<Mutex<Vec<Command>>>;

/// Snapshot of one tick: the bodies (as in the physics sandbox) and the terrain's state.
#[derive(Clone, Debug, Default)]
pub struct PlanetSnapshot {
    /// Bodies.
    pub bodies: PhysicsSnapshot,
    /// Number of edits applied so far.
    pub edits: usize,
    /// Changes whenever the edits were replaced rather than added to.
    pub edits_generation: u64,
}

/// Physics on a planet.
#[derive(Debug)]
pub struct PlanetSim {
    terrain: Arc<Terrain>,
    world: World,
    commands: Commands,
    time: f64,
    step_ms: f64,
    generation: u64,
}

impl PlanetSim {
    /// A world on `terrain`, stepped by `dt`; returns it with its command inbox.
    pub fn new(terrain: Arc<Terrain>, dt: f64) -> (Self, Commands) {
        let mut world = World::new(dt);
        world.planet = Some(planet_gravity(terrain.planet()));
        world.ground = Some(Arc::new(TerrainGround(Arc::clone(&terrain))));
        world.static_friction = 0.7;
        let commands: Commands = Arc::default();
        let sim = Self {
            terrain,
            world,
            commands: Arc::clone(&commands),
            time: 0.0,
            step_ms: 0.0,
            generation: 0,
        };
        (sim, commands)
    }

    fn apply(&mut self, command: Command) {
        let dt = self.world.dt();
        match command {
            Command::Crate(at) => {
                let half = DVec3::splat(0.5);
                let mut body = Body::rigid(Shape::Box { half }, 200.0, at, dt);
                body.structure.orientation[0] = upright(at);
                body.friction = 0.7;
                self.world.add(body);
            }
            Command::Ball(at) => {
                let mut body = Body::rigid(Shape::Sphere { radius: 0.4 }, 300.0, at, dt);
                body.friction = 0.7;
                body.restitution = 0.3;
                self.world.add(body);
            }
            Command::Edit(edit) => {
                let (lo, hi) = edit.brush().bounds();
                self.terrain.edit(edit);
                // Bodies resting on removed ground must fall.
                self.world
                    .wake_near((lo + hi) * 0.5, (hi - lo).length() * 0.5 + 1.0);
            }
            Command::LoadEdits(edits) => {
                self.terrain.set_edits(edits);
                self.generation += 1;
                for body in &mut self.world.bodies {
                    body.wake();
                }
            }
            Command::Clear => self.world.bodies.clear(),
        }
    }
}

/// A crate's orientation standing upright on the local horizon.
fn upright(at: DVec3) -> DQuat {
    DQuat::from_rotation_arc(DVec3::Y, at.normalize())
}

impl Simulation for PlanetSim {
    type Snapshot = PlanetSnapshot;

    fn step(&mut self, dt: f64) {
        let commands =
            std::mem::take(&mut *self.commands.lock().unwrap_or_else(PoisonError::into_inner));
        for command in commands {
            self.apply(command);
        }
        let start = Instant::now();
        self.world.step();
        self.step_ms = start.elapsed().as_secs_f64() * 1e3;
        self.time += dt;
    }

    fn snapshot(&self) -> PlanetSnapshot {
        let mut bodies = PhysicsSnapshot {
            time: self.time,
            step_ms: self.step_ms,
            ..PhysicsSnapshot::default()
        };
        let mut asleep = 0;
        for body in &self.world.bodies {
            asleep += usize::from(body.sleep.asleep);
            let s = &body.structure;
            for &(node, shape) in &body.shapes {
                match shape {
                    Shape::Box { half } => {
                        bodies
                            .boxes
                            .push((s.position[node], s.orientation[node], half));
                    }
                    Shape::Sphere { radius } => bodies.spheres.push((s.position[node], radius)),
                }
            }
        }
        bodies.counts = (self.world.bodies.len(), asleep, self.world.contacts.len());
        PlanetSnapshot {
            bodies,
            edits: self.terrain.edits().len(),
            edits_generation: self.generation,
        }
    }
}
