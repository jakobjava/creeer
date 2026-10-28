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
mod tests;

use crate::world::DT;
use creeer_math::glam::{DQuat, DVec3};
use creeer_net::entity::{EntityId, Kind};
use creeer_net::interp::Pose;
use creeer_net::message::AvatarInput;
use creeer_net::predict::{InputLog, Smoothing, reconcile};
use creeer_net::snapshot::Snapshot;
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

    /// The server had the world as `snapshot` says (the avatar `avatar`, the entities
    /// `kinds`), after its input `snapshot.last_input`: the avatar and the bodies near it are
    /// set to it and the inputs since are applied again (if the snapshot is newer than the
    /// last one it was set to). `drawn` gives where an entity not predicted is drawn now
    /// (interpolated): a body coming into the prediction slides from there.
    pub fn reconcile(
        &mut self,
        snapshot: &Snapshot,
        (avatar, kinds): (EntityId, &BTreeMap<EntityId, Kind>),
        drawn: impl Fn(EntityId) -> Option<DVec3>,
    ) {
        let started = std::time::Instant::now();
        let tick = snapshot.tick;
        if self.fallen || self.reconciled.is_some_and(|t| t >= tick) {
            return;
        }
        let Some(state) = snapshot.states.get(&avatar) else {
            return;
        };
        self.reconciled = Some(tick);
        // Predicted as the server had it, and the same bodies near: nothing to correct.
        let chosen: Vec<EntityId> = (nearby::chosen(snapshot, kinds, state.position()).iter())
            .map(|c| c.0)
            .collect();
        if chosen == self.nearby && self.history.agrees(snapshot, avatar, &self.nearby) {
            self.history.forget_before(snapshot.last_input);
            self.log.applied(snapshot.last_input);
            self.corrections.count += 1;
            self.corrections.replaying += started.elapsed();
            return;
        }
        let before = self.world.bodies.first().map(|b| b.structure.position[0]);
        let was: BTreeMap<EntityId, DVec3> = (self.nearby.iter())
            .zip(self.world.bodies.iter().skip(1))
            .map(|(id, b)| (*id, b.structure.position[0]))
            .collect();
        let mut world = std::mem::replace(&mut self.world, World::new(DT));
        // (The air and sea know the bodies by index: told what was kept.)
        let kept = world.retain_bodies(|i, _| i == 0);
        if let Some(a) = self.around.as_mut() {
            a.retain(&kept);
        }
        if world.bodies.is_empty() {
            world.add(character::body(
                state.position(),
                state.position().normalize(),
                DT,
            ));
        }
        let body = &mut world.bodies[0];
        body.wake();
        let s = &mut body.structure;
        s.position[0] = state.position();
        s.orientation[0] = state.orientation();
        s.velocity[0] = state.velocity();
        s.angular_velocity[0] = state.spin();
        body.solver.set_mass(
            &mut body.structure,
            0,
            character::DRY + self.gas + self.load,
        );
        body.solver.reset_history(&body.structure);
        // Asleep on the server, asleep here too, as the bodies near it are (`nearby.rs`): a
        // character standing still sleeps, and woken it would be stepped where the server's
        // stands as it fell asleep (G1.3i: standing still no longer creeps, so it does sleep).
        if state.asleep {
            body.sleep_now();
        }
        // The bodies near it, as the server had them at this input (ADR-0025).
        let last_input = snapshot.last_input;
        self.templates.retain(kinds);
        self.nearby.clear();
        for chosen in nearby::chosen(snapshot, kinds, state.position()) {
            let stale = snapshot.stale.binary_search(&chosen.0).is_ok();
            let from = (stale, &self.history, last_input);
            if self.templates.add(&mut world, chosen, from) {
                self.nearby.push(chosen.0);
            }
        }
        self.history.forget_before(last_input);
        let around = self.around.take();
        let (ids, history, load) = (&self.nearby, &mut self.history, self.load);
        let work = &mut self.corrections.work;
        (self.world, self.around) = reconcile(
            (world, around),
            last_input,
            &mut self.log,
            |(w, a), input| {
                *work += Self::step(w, a, input, load);
                history.record(input.seq, ids, w);
            },
        );
        self.smooth(before, &was, drawn);
        self.corrections.replaying += started.elapsed();
    }

    /// The drawn bodies' offsets after a correction: the avatar's and each body's slide from
    /// where it was drawn; a body coming in slides from where it was interpolated, one going
    /// out slides from where it was predicted to where it is interpolated.
    fn smooth(
        &mut self,
        before: Option<DVec3>,
        was: &BTreeMap<EntityId, DVec3>,
        drawn: impl Fn(EntityId) -> Option<DVec3>,
    ) {
        if let Some(before) = before {
            let after = self.world.bodies[0].structure.position[0];
            self.smoothing.corrected(before, after);
            // The tick before moves with the correction, so the drawn avatar stays where it was.
            if let Some(p) = &mut self.previous {
                p.position += after - before;
            }
            let c = &mut self.corrections;
            let by = (after - before).length();
            c.count += 1;
            c.over_a_centimetre += u32::from(by > 0.01);
            c.largest = c.largest.max(by);
        }
        let now: BTreeMap<EntityId, DVec3> = (self.nearby.iter())
            .zip(self.world.bodies.iter().skip(1))
            .map(|(id, b)| (*id, b.structure.position[0]))
            .collect();
        for (id, &after) in &now {
            let from = was.get(id).copied().or_else(|| drawn(*id));
            let smoothing = self.drawn.entry(*id).or_default();
            if let Some(before) = from {
                smoothing.corrected(before, after);
            }
        }
        for (id, &at) in was {
            if !now.contains_key(id) {
                let offset = self.drawn.remove(id).unwrap_or_default();
                let mut released = offset;
                if let Some(to) = drawn(*id) {
                    released.corrected(at, to);
                }
                self.released.insert(*id, released);
            }
        }
        self.drawn.retain(|id, _| now.contains_key(id));
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
