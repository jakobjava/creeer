//! Carrying and throwing (G1.4f, ADR-0027 decision 6): a player grabs what they reach (G1.4d's
//! checks) and holds it in both hands. The hands are not a rigid hold but a person's arms
//! (`hands.rs`): they hold the load's weight up as planned and pull it towards where they mean
//! to have it with an arm's measured stiffness and damping, no harder than they are strong,
//! and the same back on the body. They lift it to their height and bring it in (`creeer_sim::
//! carry::held`: at the elbows, a forearm ahead, the way the body goes), turning it as the
//! body turns. A load they cannot hold up, or that is torn out of their reach, drops. A throw
//! lets go, the load given the speed of an overarm throw of its mass (`carry::throw_speed`),
//! the thrower its reaction.

use super::*;
use creeer_math::glam::DQuat;
use creeer_net::message::Target;
use creeer_sim::carry;

/// Below this speed the body keeps facing as it did (m/s).
pub(super) const TURNING: f64 = 0.3;
/// How far a load clears the body's front, held (m).
const CLEAR: f64 = 0.01;

/// A load carried.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Carry {
    pub(super) load: EntityId,
    pub(super) node: usize,
    /// The point the hands hold, in the load's node frame.
    pub(super) grip: DVec3,
    /// Its turn from the way the body faced (`facing_frame`) when taken: kept as it turns.
    pub(super) turn: DQuat,
    /// Where the hands took it, in the body's frame, how long they take to lift it (s), and
    /// the ticks since.
    pub(super) from: DVec3,
    pub(super) lift: f64,
    pub(super) since: u32,
    /// The way the body faces (horizontal); how fast it turns (rad/s, about up), and the
    /// hands' spare strength (m/s² of the load: half of what its weight leaves).
    pub(super) facing: DVec3,
    pub(super) spin: f64,
    pub(super) spare: f64,
    /// How far before the body's axis the hands hold it (m).
    pub(super) ahead: f64,
    /// Where the hands meant to have it the last two ticks, in the body's frame.
    pub(super) path: [Option<DVec3>; 2],
    /// The ticks running the hands have given their most, and have had it beyond their give.
    pub(super) straining: u32,
    pub(super) torn: u32,
}

/// The frame of a body facing `facing` (horizontal) about `up`: z the way it faces, y up.
pub(super) fn facing_frame(facing: DVec3, up: DVec3) -> DQuat {
    let z = facing
        .reject_from(up)
        .normalize_or(up.any_orthonormal_vector());
    let x = up.cross(z).normalize();
    DQuat::from_mat3(&creeer_math::glam::DMat3::from_cols(x, up.normalize(), z)).normalize()
}

impl ServerWorld {
    /// Avatar `by` grabs `target` (its tick the world's), putting away the tool in hand.
    pub(super) fn grab(&mut self, by: EntityId, target: &Target) -> Result<(), String> {
        target.body.ok_or("nothing to hold on the ground")?;
        let reached = self.reached(by, target.body, target.at, target.seen)?;
        let load = reached.body.ok_or("nothing to hold")?;
        self.take_hold(by, load, reached.point)
    }

    /// Avatar `by` takes body `load`'s node in both hands, gripping it at `point`.
    pub(super) fn take_hold(
        &mut self,
        by: EntityId,
        (load, node): (usize, usize),
        point: DVec3,
    ) -> Result<(), String> {
        let i = self.index(by).ok_or("no such avatar")?;
        let player = self.players[i].clone().ok_or("only a player grabs")?;
        if self.people.get(&by).is_some_and(people::Person::fallen) {
            return Err("down".into());
        }
        if self.carrying.contains_key(&by) {
            return Err("hands full".into());
        }
        let id = self.ids[load];
        if self.players[load].is_some() || self.carrying.values().any(|c| c.load == id) {
            return Err("not to be carried".into());
        }
        // Lifted with at most half the strength its weight leaves the hands, which move the arms
        // too.
        let mass = self.mass_of(load);
        let spare = carry::spare(mass);
        if spare <= 0.0 {
            return Err(format!("{mass:.0} kg: too heavy to lift"));
        }
        if self.wields.remove(&player).is_some() {
            self.changed.push(by);
        }
        let s = &self.world.bodies[i].structure;
        let (centre, turn) = (s.position[0], s.orientation[0]);
        let up = character::plumb(&self.world, centre);
        let facing = (point - centre)
            .reject_from(up)
            .normalize_or(up.any_orthonormal_vector());
        let l = &self.world.bodies[load].structure;
        let (c, q) = (l.position[node], l.orientation[node]);
        let grip = q.conjugate() * (point - c);
        let from = turn.conjugate() * (point - centre);
        let ahead = self.ahead((load, node), point, i, facing);
        // Each half of the way (up, then in) as fast as the spare strength lets: smoothly,
        // its most acceleration 24·d/T².
        let held = turn.conjugate() * (carry::held_out(centre, up, facing, ahead) - centre);
        let up_here = turn.conjugate() * up;
        let rise = (held - from).dot(up_here).abs();
        let inward = (held - from).reject_from(up_here).length();
        let lift = (24.0 * rise.max(inward) / spare).sqrt();
        self.carrying.insert(
            by,
            Carry {
                load: id,
                node,
                grip,
                turn: facing_frame(facing, up).conjugate() * q,
                from,
                lift,
                since: 0,
                facing,
                spin: 0.0,
                spare,
                ahead,
                path: [None; 2],
                straining: 0,
                torn: 0,
            },
        );
        // The load in the hands passes the body's box (as a joint's sides did; the box is no
        // chest and arms: a load swaying in soft hands struck it and tumbled).
        self.world.let_pass(i, load);
        // (Told anew, the load is no longer predicted on its own: in the hands.)
        self.changed.extend([by, id]);
        Ok(())
    }

    /// Lets go of the load, if any.
    pub(super) fn let_go(&mut self, by: EntityId) {
        if let Some(c) = self.carrying.remove(&by) {
            if let (Some(a), Some(b)) = (self.index(by), self.index(c.load)) {
                self.world.let_collide(a, b);
            }
            self.changed.extend([by, c.load]);
        }
    }

    /// Avatar `by` throws its load along `direction`.
    pub(super) fn throw(&mut self, by: EntityId, direction: DVec3) -> Result<(), String> {
        if !direction.is_finite() || direction.length_squared() < 1e-12 {
            return Err("no way to throw".into());
        }
        let c = self.carrying.get(&by).ok_or("nothing in hand")?;
        let (a, b, node) = (
            self.index(by).ok_or("no such avatar")?,
            self.index(c.load).ok_or("gone")?,
            c.node,
        );
        let point = self.world.bodies[b].structure.position[node];
        self.let_go(by);
        let mass = self.mass_of(b);
        let thrower = self.world.bodies[a].structure.velocity[0];
        let now = self.world.bodies[b].structure.velocity[node];
        let want = thrower + direction.normalize() * carry::throw_speed(mass);
        let impulse = (want - now) * mass;
        self.world.apply_impulse(b, node, impulse, point);
        let centre = self.world.bodies[a].structure.position[0];
        self.world.apply_impulse(a, 0, -impulse, centre);
        Ok(())
    }

    /// What avatar `by` carries, if anything.
    pub fn carried(&self, by: EntityId) -> Option<EntityId> {
        self.carrying.get(&by).map(|c| c.load)
    }

    /// Whether entity `id` is carried by someone (not then predicted by a client on its own).
    pub fn in_hands(&self, id: EntityId) -> bool {
        self.carrying.values().any(|c| c.load == id)
    }

    /// Whether avatar `by`'s hands have lifted their load to where they hold it (for scripts:
    /// people lift, then walk off).
    pub fn lifted(&self, by: EntityId) -> bool {
        self.carrying
            .get(&by)
            .is_some_and(|c| f64::from(c.since) * DT >= c.lift)
    }

    /// After a save is loaded (who carried what is not saved): the joints the hands held with
    /// before G1.4f's compliant hands let go: any joint between an avatar and a body not its own
    /// ragdoll's.
    pub(super) fn let_go_after_load(&mut self) {
        let parts: Vec<usize> = (self.people.values())
            .flat_map(|p| p.parts.iter())
            .filter_map(|id| self.index(*id))
            .collect();
        let avatar = |i: usize| self.players[i].is_some();
        let held: Vec<usize> = (self.world.joints().iter().enumerate())
            .filter(|(_, j)| {
                let b = j.b.map(|b| b.0);
                (avatar(j.a.0) && b.is_some_and(|b| !parts.contains(&b)))
                    || (b.is_some_and(avatar) && !parts.contains(&j.a.0))
            })
            .map(|(k, _)| k)
            .collect();
        for k in held.into_iter().rev() {
            self.world.remove_joint(k);
        }
        // And the loads the hands held passing their bodies meet them again (a world handed
        // over whole, not read from bytes, keeps the pairs let pass).
        for a in (0..self.players.len()).filter(|&a| avatar(a)) {
            for b in (0..self.world.bodies.len()).filter(|b| !parts.contains(b) && *b != a) {
                self.world.let_collide(a, b);
            }
        }
    }

    /// Body `index`'s mass (kg): its nodes'.
    pub(super) fn mass_of(&self, index: usize) -> f64 {
        let s = &self.world.bodies[index].structure;
        (0..s.node_count())
            .map(|n| self.world.effective_mass(index, n, s.position[n], DVec3::X))
            .sum()
    }

    /// How far before the body's axis (of body `a`) the hands hold the load (`load`'s node,
    /// taken at `grab`), facing `facing` (m): a forearm, or farther if the load would not then
    /// clear the body's front (a load held inside the body burst out of it when let go).
    fn ahead(&self, (load, node): (usize, usize), grab: DVec3, a: usize, facing: DVec3) -> f64 {
        let body = &self.world.bodies[load];
        let (c, q) = (
            body.structure.position[node],
            body.structure.orientation[node],
        );
        let a = &self.world.bodies[a];
        let extent = |shapes: &[(usize, creeer_physics::world::shape::Shape)], turn, way| {
            (shapes.iter())
                .map(|(_, shape)| shape.extent(turn, way))
                .fold(0.0, f64::max)
        };
        // The load's side towards the body, and the body's own front that way (its box never
        // turns: its side, or its front).
        let near = extent(&body.shapes, q, -facing);
        let front = extent(&a.shapes, a.structure.orientation[0], facing);
        (front + CLEAR + near + (grab - c).dot(facing)).max(carry::forearm())
    }
}
