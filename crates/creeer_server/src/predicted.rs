//! The player's own avatar as the client predicts it, with the bodies near it (G1.2e,
//! ADR-0025): on the terrain in a physics world of the client's, in the planet's air and sea
//! as the server's is (swimming is predicted), steered by each input as it is made
//! (`creeer_net::predict`), and set back to the server's state whenever a snapshot says where
//! the server has it, the inputs the server has not yet applied applied again. The bodies
//! within a few metres of it are set back and replayed with it (`predicted/nearby.rs`), so
//! walking into a crate is predicted. What the server knows and the client does not (a body
//! further off, another player's push) shows as a correction, which the drawn bodies slide
//! through.

mod nearby;
#[cfg(test)]
mod probe_tests;
mod rewind;
mod smooth;
#[cfg(test)]
mod tests;

use crate::world::DT;
use creeer_math::glam::{DQuat, DVec3};
use creeer_net::entity::EntityId;
use creeer_net::interp::Pose;
use creeer_net::message::AvatarInput;
use creeer_net::predict::{InputLog, Smoothing};
use creeer_physics::world::World;
use creeer_sim::surroundings::Surroundings;
use creeer_sim::{TerrainGround, character, planet_gravity};
use creeer_terrain::Terrain;
use nearby::{History, Templates};
use std::collections::BTreeMap;
use std::sync::Arc;

/// The predicted avatar and the bodies near it.
#[derive(Debug)]
pub struct Predicted {
    /// Body 0 the avatar, then the bodies near it (`nearby`, in order).
    world: World,
    /// The planet's air and sea on it (as on the server's).
    around: Option<Surroundings>,
    log: InputLog,
    smoothing: Smoothing,
    /// The avatar as it was before its last tick, and the time since that tick (s): drawn
    /// between the two by the share of a tick passed, so it moves smoothly on a screen drawing
    /// faster than it ticks (render interpolation: Fiedler, "Fix Your Timestep!", 2004).
    previous: Option<Pose>,
    since: f64,
    /// The tick of the snapshot it was last set back to.
    reconciled: Option<u32>,
    /// How far the avatar was corrected each time it was set back (m).
    pub corrections: Corrections,
    /// The jetpack's gas and what the pockets hold as the server last told them (kg): their
    /// mass is the body's.
    gas: f64,
    load: f64,
    /// Whether the character is down (its ragdoll is the server's alone: not predicted).
    fallen: bool,
    /// The bodies predicted with it, and each one's drawn offset (corrections and coming in).
    nearby: Vec<EntityId>,
    drawn: BTreeMap<EntityId, Smoothing>,
    /// Bodies that left the prediction: the offset they are drawn at from their interpolated
    /// pose, while it slides away.
    released: BTreeMap<EntityId, Smoothing>,
    history: History,
    templates: Templates,
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
    /// The time spent setting the prediction back and replaying it, in all.
    pub replaying: std::time::Duration,
    /// The Newton iterations its ticks took, replays included (the client's work, as a play
    /// scene's server's is counted).
    pub work: u64,
    /// Of the times set back, those that were rewound and replayed (the rest agreed with the
    /// snapshot: nothing to replay), the inputs replayed in them, and their Newton iterations.
    pub rewinds: u32,
    pub replayed: u32,
    pub replay_work: u64,
    /// Why they were rewound: the avatar mispredicted; it did not, but a body near it did; or
    /// neither, but the bodies to predict changed.
    pub for_avatar: u32,
    pub for_bodies: u32,
    pub for_choice: u32,
    /// How far off the avatar and the bodies were at the rewinds (the most of any body): counts
    /// under 2, 5, 10, 30 and 100 mm, and beyond.
    pub avatar_off: [u32; 6],
    pub bodies_off: [u32; 6],
    /// What was off at the rewinds, the avatar's and any body's: counts of position, velocity,
    /// spin and orientation beyond twice the snapshots' quanta.
    pub off_in: [[u32; 4]; 2],
}

impl Corrections {
    /// Counts a rewind's errors: the avatar's and the most of any body near it (m).
    fn count_off(&mut self, avatar: f64, bodies: f64) {
        let bucket = |m: f64| {
            [0.002, 0.005, 0.01, 0.03, 0.1]
                .iter()
                .position(|limit| m < *limit)
                .unwrap_or(5)
        };
        self.avatar_off[bucket(avatar)] += 1;
        self.bodies_off[bucket(bodies)] += 1;
    }
}

impl Predicted {
    /// A prediction on `terrain`, in `around` (the avatar comes with the first snapshot that
    /// has it).
    pub fn new(terrain: Arc<Terrain>, around: Option<Surroundings>) -> Self {
        let mut world = World::new(DT);
        world.planet = Some(planet_gravity(terrain.planet()));
        world.static_friction = 0.7;
        world.ground = Some(Arc::new(TerrainGround(terrain)));
        Self {
            world,
            around,
            log: InputLog::default(),
            smoothing: Smoothing::default(),
            previous: None,
            since: 0.0,
            reconciled: None,
            corrections: Corrections::default(),
            gas: character::TANK,
            load: 0.0,
            fallen: false,
            nearby: Vec::new(),
            drawn: BTreeMap::new(),
            released: BTreeMap::new(),
            history: History::default(),
            templates: Templates::default(),
        }
    }

    /// One tick of `input` on `world`: the Newton iterations it took.
    fn step(
        world: &mut World,
        around: &mut Option<Surroundings>,
        input: &AvatarInput,
        load: f64,
    ) -> u64 {
        let at = world.bodies[0].structure.position[0];
        let water = (around.as_ref().and_then(|a| a.sea.as_ref())).map(|sea| sea.depth(at));
        let mut reactions = Vec::new();
        let act = character::Intent {
            walk: input.walk,
            jump: input.jump,
            thrust: input.thrust,
            load,
            stopping: input.stopping,
        };
        let up = character::plumb(world, at);
        character::steer(world, 0, act, (up, water), &mut reactions);
        if let Some(a) = around.as_mut() {
            a.push(world);
        }
        let work = |w: &World| -> u64 { w.bodies.iter().map(|b| b.solver.iterations_total).sum() };
        let before = work(world);
        world.step();
        let worked = work(world).saturating_sub(before);
        character::release(world, &mut reactions);
        if let Some(a) = around.as_mut() {
            a.settle(world, DT);
        }
        worked
    }

    /// The fixed shapes it meets (G1.7: trunks), in place of the last.
    pub fn set_statics(&mut self, statics: Vec<creeer_physics::world::shape::StaticShape>) {
        self.world.statics = statics;
    }

    /// A new input (one a tick): kept, applied to the avatar at once, and returned to be sent.
    /// Steers the hands to `hold` from the next input on (G2.10; none: as they hold by
    /// themselves).
    pub fn set_hold(&mut self, hold: Option<creeer_net::message::Hold>) {
        self.log.set_hold(hold);
    }

    pub fn input(&mut self, walk: DVec3, jump: bool, thrust: DVec3) -> AvatarInput {
        let input = self.log.push(walk, jump, thrust);
        if !self.fallen && !self.world.bodies.is_empty() {
            self.previous = self.pose_with(DVec3::ZERO);
            self.since = 0.0;
            self.corrections.work +=
                Self::step(&mut self.world, &mut self.around, &input, self.load);
            self.history.record(input.seq, &self.nearby, &self.world);
        }
        input
    }

    /// The jetpack's gas, as the server tells it (kg): set at the next correction.
    pub fn set_gas(&mut self, gas: f64) {
        self.gas = gas;
    }

    /// What the pockets hold, as the server tells it (kg): set at the next correction.
    pub fn set_load(&mut self, load: f64) {
        self.load = load;
    }

    /// Whether the character is down, as the server tells it. Up again, it is predicted afresh
    /// from the next snapshot (no correction from where it fell).
    pub fn set_fallen(&mut self, fallen: bool) {
        if self.fallen && !fallen && !self.world.bodies.is_empty() {
            let kept = self.world.retain_bodies(|_, _| false);
            if let Some(a) = self.around.as_mut() {
                a.retain(&kept);
            }
            self.nearby.clear();
            self.drawn.clear();
            self.smoothing = Smoothing::default();
            self.previous = None;
        }
        self.fallen = fallen;
    }

    /// The inputs to send this tick.
    pub fn to_send(&self) -> Vec<AvatarInput> {
        self.log.to_send()
    }

    /// `seconds` passed: the avatar as drawn (the prediction, less what is left of the last
    /// corrections), if it has come yet.
    pub fn advance(&mut self, seconds: f64) -> Option<Pose> {
        self.since += seconds;
        let offset = self.smoothing.advance(seconds);
        for s in self.drawn.values_mut().chain(self.released.values_mut()) {
            s.advance(seconds);
        }
        self.released.retain(|_, s| s.advance(0.0).length() > 1e-4);
        self.pose_with(offset)
    }

    /// The avatar as drawn now.
    pub fn pose(&self) -> Option<Pose> {
        let mut smoothing = self.smoothing;
        self.pose_with(smoothing.advance(0.0))
    }

    fn pose_with(&self, offset: DVec3) -> Option<Pose> {
        let s = &self.world.bodies.first()?.structure;
        let (mut position, mut orientation) = (s.position[0], s.orientation[0]);
        if let Some(p) = &self.previous {
            let share = (self.since / DT).clamp(0.0, 1.0);
            position = p.position.lerp(position, share);
            orientation = p.orientation.slerp(orientation, share);
        }
        Some(Pose {
            position: position + offset,
            orientation,
        })
    }

    /// The bodies predicted with the avatar, as drawn now.
    pub fn nearby(&self) -> impl Iterator<Item = (EntityId, Pose)> + '_ {
        (self.nearby.iter())
            .zip(self.world.bodies.iter().skip(1))
            .map(|(id, b)| {
                let mut smoothing = self.drawn.get(id).copied().unwrap_or_default();
                let s = &b.structure;
                let pose = Pose {
                    position: s.position[0] + smoothing.advance(0.0),
                    orientation: s.orientation[0],
                };
                (*id, pose)
            })
    }

    /// Where a body that left the prediction is drawn from its interpolated pose.
    pub fn released(&self, id: EntityId) -> DVec3 {
        (self.released.get(&id)).map_or(DVec3::ZERO, |s| s.clone().advance(0.0))
    }

    /// The avatar's velocity as predicted (m/s).
    pub fn velocity(&self) -> DVec3 {
        (self.world.bodies.first()).map_or(DVec3::ZERO, |b| b.structure.velocity[0])
    }

    /// Its orientation, if it has come.
    pub fn orientation(&self) -> Option<DQuat> {
        Some(self.world.bodies.first()?.structure.orientation[0])
    }

    /// The server tick the avatar is predicted at: the snapshot it was last set back to, and a
    /// tick for each input applied since (none before the first snapshot).
    pub fn tick(&self) -> Option<u32> {
        let pending = u32::try_from(self.log.pending().count()).ok()?;
        Some(self.reconciled? + pending)
    }
}
