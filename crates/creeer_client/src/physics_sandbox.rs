//! F2 physics sandbox: scenes that exercise the physics core (structures, rigid bodies,
//! contacts, plasticity) on the simulation thread.

use crate::physics_heat::{self, SceneHeat};
use creeer_core::Simulation;
use creeer_math::glam::{DMat3, DQuat, DVec3};
use creeer_physics::structure::Section;
use creeer_physics::structure::section::STEEL_S235;
use creeer_physics::structure::shapes::{Base, space_frame};
use creeer_physics::world::shape::{Shape, StaticShape};
use creeer_physics::world::{Body, World};
use std::time::Instant;

/// The scenes on offer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhysicsScene {
    /// A 40 m welded steel bridge resting on two piers; a 10 t load lands on it.
    Bridge,
    /// A 1.2 t vehicle frame of welded steel tubes drives into a concrete wall at 15 m/s.
    Crash,
    /// Stacks of crates, hit by a heavy ball.
    Stacks,
    /// Rocks (convex hulls) dropped onto a ramp, a hexagonal prism and wedges at its foot.
    Rocks,
    /// Steel blocks at 900 °C cooling, one in air, one in a vacuum (F6.2).
    Cooling,
    /// A radiator panel in orbit turning in and out of sunlight (F6.2).
    Radiator,
    /// A loaded steel frame in a fire, sagging as it weakens (F6.2).
    Fire,
}

impl PhysicsScene {
    /// All scenes with their names.
    pub const ALL: [(Self, &'static str); 7] = [
        (Self::Bridge, "bridge under load"),
        (Self::Crash, "crash test"),
        (Self::Stacks, "stacking"),
        (Self::Rocks, "rock pile"),
        (Self::Cooling, "cooling blocks"),
        (Self::Radiator, "radiator in orbit"),
        (Self::Fire, "frame in a fire"),
    ];
}

/// The physics world of one scene.
#[derive(Debug)]
pub struct PhysicsSandbox {
    world: World,
    time: f64,
    step_ms: f64,
    scene: PhysicsScene,
    heat: Option<SceneHeat>,
}

/// How a box is drawn.#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]pub enum Look {    /// A wooden crate.    #[default]    Crate,    /// Bare steel (glowing when hot).    Steel,    /// A white-painted radiator panel.    Radiator,}
/// How a box is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Look {
    /// A wooden crate.
    #[default]
    Crate,
    /// Bare steel (glowing when hot).
    Steel,
    /// A concrete block.
    Concrete,
    /// A white-painted radiator panel.
    Radiator,
}

/// What the renderer needs from one tick.
#[derive(Clone, Debug, Default)]
pub struct PhysicsSnapshot {
    /// Moving boxes: centre, orientation, half extents.
    pub boxes: Vec<(DVec3, DQuat, DVec3)>,
    /// Balls: centre, radius.
    pub spheres: Vec<(DVec3, f64)>,
    /// Hulls: centre, orientation, which of the catalogue (`hulls.rs`).
    pub hulls: Vec<(DVec3, DQuat, usize)>,
    /// Members: ends, radius, utilisation (0 unloaded, 1 yielding), broken.
    pub beams: Vec<(DVec3, DVec3, f64, f32, bool)>,
    /// Fixed boxes (walls, piers): centre, orientation, half extents.
    pub walls: Vec<(DVec3, DQuat, DVec3)>,
    /// Simulated time (s).
    pub time: f64,
    /// Bodies, of which asleep, and contacts.
    pub counts: (usize, usize, usize),
    /// Each box's temperature (°C) where heat follows it (else NaN), and how it looks.
    pub box_heat: Vec<f32>,
    pub box_look: Vec<Look>,
    /// Each member's temperature (°C) where heat follows it, else NaN.
    pub beam_heat: Vec<f32>,
    /// A fire's bed (centre, half extents) and its gas temperature (°C).
    pub fire: Option<(DVec3, DVec3, f32)>,
    /// What heat is doing, for the window.
    pub heat: Option<String>,
    /// Cost of the last physics step (ms).
    pub step_ms: f64,
}

impl PhysicsSandbox {
    /// Builds `scene`, stepped at `dt` seconds.
    pub fn new(scene: PhysicsScene, dt: f64) -> Self {
        let mut world = World::new(dt);
        world.statics.push(StaticShape::Plane {
            normal: DVec3::Y,
            offset: 0.0,
        });
        match scene {
            PhysicsScene::Bridge => bridge(&mut world),
            PhysicsScene::Crash => crash(&mut world),
            PhysicsScene::Stacks => stacks(&mut world),
            PhysicsScene::Rocks => crate::physics_rocks::rocks(&mut world),
            _ => {}
        }
        let heat = match scene {
            PhysicsScene::Cooling => Some(physics_heat::blocks(&mut world)),
            PhysicsScene::Radiator => {
                world.statics.clear();
                let sun = crate::demo::sun_direction(crate::DEFAULT_SUN_ELEVATION);
                Some(physics_heat::radiator(&mut world, sun))
            }
            PhysicsScene::Fire => Some(physics_heat::fire(&mut world)),
            _ => None,
        };
        Self {
            world,
            time: 0.0,
            step_ms: 0.0,
            scene,
            heat,
        }
    }
}

pub(crate) fn wall(world: &mut World, center: DVec3, half: DVec3) {
    world.statics.push(StaticShape::Box {
        center,
        rotation: DQuat::IDENTITY,
        half,
    });
}

fn bridge(world: &mut World) {
    let dt = world.dt();
    let tube = Section::round_tube(0.16, 0.008);
    let mut deck = space_frame([20, 1, 2], 2.0, tube, STEEL_S235, Base::Free);
    // Simply supported: the bottom joints (balls of the tube's radius) rest on the piers.
    deck.translate(DVec3::new(-20.0, 4.0 + tube.extent[0], -2.0));
    world.add(Body::structure(deck, dt));
    for x in [-20.5, 20.5] {
        wall(world, DVec3::new(x, 2.0, 0.0), DVec3::new(1.0, 2.0, 3.0));
    }
    // A 4.4 m square load over nine deck joints, dropped from 0.5 m.
    let deck_top = 6.0 + 2.0 * tube.extent[0];
    let block = Body::rigid(
        Shape::Box {
            half: DVec3::new(2.2, 0.5, 2.2),
        },
        10_000.0,
        DVec3::new(0.0, deck_top + 1.0, 0.0),
        dt,
    );
    world.add(block);
}

fn crash(world: &mut World) {
    let dt = world.dt();
    let tube = Section::round_tube(0.08, 0.004);
    let mut car = space_frame([2, 1, 1], 2.0, tube, STEEL_S235, Base::Free);
    // Engine, body and cargo: 1.2 t spread over the joints.
    for node in 0..car.node_count() {
        car.add_mass(node, 100.0, DMat3::ZERO);
    }
    car.translate(DVec3::new(-3.0, tube.extent[0] + 0.005, -1.0));
    for v in &mut car.velocity {
        *v = DVec3::X * 15.0;
    }
    world.add(Body::structure(car, dt));
    wall(world, DVec3::new(3.0, 1.5, 0.0), DVec3::new(0.5, 1.5, 4.0));
}

fn stacks(world: &mut World) {
    let dt = world.dt();
    for stack in 0..5 {
        for level in 0..6 {
            let position = DVec3::new(f64::from(stack) * 1.4 - 2.8, 0.5 + f64::from(level), 0.0);
            let mut crate_ = Body::rigid(
                Shape::Box {
                    half: DVec3::splat(0.5),
                },
                80.0,
                position,
                dt,
            );
            crate_.friction = 0.6;
            world.add(crate_);
        }
    }
    let mut ball = Body::rigid(
        Shape::Sphere { radius: 0.6 },
        800.0,
        DVec3::new(-2.8, 3.5, -12.0),
        dt,
    );
    ball.structure.velocity[0] = DVec3::new(0.0, 2.0, 12.0);
    ball.restitution = 0.3;
    world.add(ball);
}

impl Simulation for PhysicsSandbox {
    type Snapshot = PhysicsSnapshot;

    fn step(&mut self, dt: f64) {
        let start = Instant::now();
        self.world.step();
        if let Some(heat) = &mut self.heat {
            heat.tick(&mut self.world, dt);
        }
        self.time += dt;
        self.step_ms = start.elapsed().as_secs_f64() * 1000.0;
    }

    fn snapshot(&self) -> PhysicsSnapshot {
        let mut snap = PhysicsSnapshot {
            time: self.time,
            step_ms: self.step_ms,
            counts: (
                self.world.bodies.len(),
                self.world.sleeping(),
                self.world.contacts.len(),
            ),
            ..PhysicsSnapshot::default()
        };
        let heat = self.heat.as_ref();
        #[allow(clippy::cast_possible_truncation)] // colour values
        let shown = |t: Option<f64>| t.map_or(f32::NAN, |t| t as f32);
        for (index, body) in self.world.bodies.iter().enumerate() {
            let warm = heat.and_then(|h| h.body_temperature(index));
            let s = &body.structure;
            for (node, shape) in &body.shapes {
                let at = (s.position[*node], s.orientation[*node]);
                match shape {
                    Shape::Box { half } => {
                        snap.boxes.push((at.0, at.1, *half));
                        snap.box_heat.push(shown(warm));
                        snap.box_look.push(match (self.scene, warm) {
                            (PhysicsScene::Cooling, Some(_)) => Look::Steel,
                            (PhysicsScene::Radiator, Some(_)) => Look::Radiator,
                            (PhysicsScene::Fire, _) => Look::Concrete,
                            _ => Look::Crate,
                        });
                    }
                    Shape::Sphere { radius } if s.element_count() == 0 => {
                        snap.spheres.push((at.0, *radius));
                    }
                    Shape::Sphere { .. } => {}
                    Shape::Hull(hull) => {
                        if let Some(k) = crate::hulls::index_of(hull) {
                            snap.hulls.push((at.0, at.1, k));
                        }
                    }
                }
            }
            let usage = s.utilization();
            let members = heat.and_then(|h| h.member_temperatures(index, s.element_count()));
            snap.beam_heat
                .extend((0..s.element_count()).map(|k| shown(members.as_ref().map(|m| m[k]))));
            for ((a, b, radius, broken), u) in s.members().into_iter().zip(usage) {
                #[allow(clippy::cast_possible_truncation)] // a colour value
                snap.beams
                    .push((s.position[a], s.position[b], radius, u as f32, broken));
            }
        }
        if let Some(gas) = heat.and_then(SceneHeat::fire) {
            snap.fire = Some((
                DVec3::new(0.0, 0.03, 0.0),
                DVec3::new(1.0, 0.03, 0.6),
                shown(Some(gas)),
            ));
        }
        snap.heat = heat.map(SceneHeat::label);
        for fixed in &self.world.statics {
            if let StaticShape::Box {
                center,
                rotation,
                half,
            } = *fixed
            {
                snap.walls.push((center, rotation, half));
            }
        }
        snap
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_scene_runs_and_settles_without_blowing_up() {
        for (scene, name) in PhysicsScene::ALL {
            let mut sandbox = PhysicsSandbox::new(scene, 1.0 / 60.0);
            for _ in 0..180 {
                sandbox.step(1.0 / 60.0);
            }
            let snap = sandbox.snapshot();
            let finite = snap
                .beams
                .iter()
                .all(|b| b.0.is_finite() && b.1.is_finite())
                && snap.boxes.iter().all(|b| b.0.is_finite())
                && snap.hulls.iter().all(|h| h.0.is_finite());
            assert!(finite, "{name}: non-finite state");
            assert!(
                snap.boxes.iter().all(|b| b.0.y > -0.1) && snap.hulls.iter().all(|h| h.0.y > -0.1),
                "{name}: something fell through the ground"
            );
            if scene == PhysicsScene::Rocks {
                assert_eq!(snap.hulls.len(), 22, "every hull is drawn");
            }
        }
    }
}
