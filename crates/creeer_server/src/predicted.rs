//! The player's own avatar as the client predicts it: alone on the terrain in a physics world
//! of the client's, steered by each input as it is made (`creeer_net::predict`), and set
//! back to the server's state whenever a snapshot says where the server has it, the inputs
//! the server has not yet applied applied again. What the server knows and the client does
//! not (a crate it rolled into) shows as a correction, which the drawn avatar slides through.

use crate::world::DT;
use creeer_math::glam::{DQuat, DVec3};
use creeer_net::interp::Pose;
use creeer_net::message::AvatarInput;
use creeer_net::predict::{InputLog, Smoothing, reconcile};
use creeer_net::quant::BodyState;
use creeer_physics::world::World;
use creeer_sim::{TerrainGround, avatar, planet_gravity};
use creeer_terrain::Terrain;
use std::sync::Arc;

/// The predicted avatar.
#[derive(Debug)]
pub struct Predicted {
    world: World,
    log: InputLog,
    smoothing: Smoothing,
    /// The tick of the snapshot it was last set back to.
    reconciled: Option<u32>,
    /// How far it was corrected each time it was set back (m).
    pub corrections: Corrections,
}

/// How far a prediction was corrected.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Corrections {
    /// Times set back.
    pub count: u32,
    /// Times by more than a centimetre.
    pub over_a_centimetre: u32,
    /// The largest (m).
    pub largest: f64,
}

impl Predicted {
    /// A prediction on `terrain` (the avatar comes with the first snapshot that has it).
    pub fn new(terrain: Arc<Terrain>) -> Self {
        let mut world = World::new(DT);
        world.planet = Some(planet_gravity(terrain.planet()));
        world.static_friction = 0.7;
        world.ground = Some(Arc::new(TerrainGround(terrain)));
        Self {
            world,
            log: InputLog::default(),
            smoothing: Smoothing::default(),
            reconciled: None,
            corrections: Corrections::default(),
        }
    }

    fn step(world: &mut World, input: &AvatarInput) {
        let up = world.bodies[0].structure.position[0].normalize();
        avatar::steer(world, 0, (input.walk, input.jump), up);
        world.step();
    }

    /// A new input (one a tick): kept, applied to the avatar at once, and returned to be sent.
    pub fn input(&mut self, walk: DVec3, jump: bool) -> AvatarInput {
        let input = self.log.push(walk, jump);
        if !self.world.bodies.is_empty() {
            Self::step(&mut self.world, &input);
        }
        input
    }

    /// The inputs to send this tick.
    pub fn to_send(&self) -> Vec<AvatarInput> {
        self.log.to_send()
    }

    /// The server had the avatar in `state` at tick `tick`, after its input `last_input`:
    /// the avatar is set to it and the inputs since are applied again (if the snapshot is
    /// newer than the last one it was set to).
    pub fn reconcile(&mut self, tick: u32, state: &BodyState, last_input: u32) {
        if self.reconciled.is_some_and(|t| t >= tick) {
            return;
        }
        self.reconciled = Some(tick);
        let before = self.world.bodies.first().map(|b| b.structure.position[0]);
        let mut world = std::mem::replace(&mut self.world, World::new(DT));
        if world.bodies.is_empty() {
            world.add(avatar::body(state.position(), DT));
        }
        let body = &mut world.bodies[0];
        body.wake();
        let s = &mut body.structure;
        s.position[0] = state.position();
        s.orientation[0] = state.orientation();
        s.velocity[0] = state.velocity();
        s.angular_velocity[0] = state.spin();
        body.solver.reset_history(&body.structure);
        self.world = reconcile(world, last_input, &mut self.log, Self::step);
        if let Some(before) = before {
            let after = self.world.bodies[0].structure.position[0];
            self.smoothing.corrected(before, after);
            let c = &mut self.corrections;
            let by = (after - before).length();
            c.count += 1;
            c.over_a_centimetre += u32::from(by > 0.01);
            c.largest = c.largest.max(by);
        }
    }

    /// `seconds` passed: the avatar as drawn (the prediction, less what is left of the last
    /// corrections), if it has come yet.
    pub fn advance(&mut self, seconds: f64) -> Option<Pose> {
        let offset = self.smoothing.advance(seconds);
        self.pose_with(offset)
    }

    /// The avatar as drawn now.
    pub fn pose(&self) -> Option<Pose> {
        let mut smoothing = self.smoothing;
        self.pose_with(smoothing.advance(0.0))
    }

    fn pose_with(&self, offset: DVec3) -> Option<Pose> {
        let s = &self.world.bodies.first()?.structure;
        Some(Pose {
            position: s.position[0] + offset,
            orientation: s.orientation[0],
        })
    }

    /// The avatar's velocity as predicted (m/s).
    pub fn velocity(&self) -> DVec3 {
        self.world
            .bodies
            .first()
            .map_or(DVec3::ZERO, |b| b.structure.velocity[0])
    }

    /// Its orientation, if it has come.
    pub fn orientation(&self) -> Option<DQuat> {
        Some(self.world.bodies.first()?.structure.orientation[0])
    }
}
