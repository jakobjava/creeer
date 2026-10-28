//! Carrying and throwing (G1.4f, ADR-0027; held firmly since G2.10, ADR-0036): a player grabs
//! what they reach (G1.4d's checks) and holds it at the point grabbed, in one hand or two as
//! its weight and its lever about the hand need. The hands grip it firmly (`hands.rs`: a grip
//! solved with the physics, as stiff as a hand moving what it means to, capped by the hands'
//! strength and the wrist's or the two hands' torque), where the player steers it (the view,
//! the wheel, the turning keys: `creeer_net::message::Hold`), or by themselves before the body.
//! A load too heavy lags and sags; it drops only when torn out of their reach. A throw lets
//! go, the load given the speed of an overarm throw of its mass (`carry::throw_speed`), the
//! thrower its reaction.

use super::*;
use creeer_math::glam::DQuat;
use creeer_net::message::{Hold, Target};
use creeer_sim::carry;

/// Below this speed the body keeps facing as it did (m/s).
pub(super) const TURNING: f64 = 0.3;
/// How far a load clears the body's front, held (m).
const CLEAR: f64 = 0.01;
/// The most one hand carries comfortably (kg: men's one-handed carrying, 9–10 kg) and the
/// wrist's torque (N·m: radial deviation 11.0, flexion 12.2, Delp et al.): a load heavier, or
/// levered more about the hand, takes both.
const ONE_HAND: f64 = 9.0;
pub(super) const WRIST: f64 = 11.0;

/// A load carried.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Carry {
    pub(super) load: EntityId,
    pub(super) node: usize,
    /// The point the hands hold, in the load's node frame (moved along a slender load's axis
    /// hand over hand: `hands.rs`).
    pub(super) grip: DVec3,
    /// Its turn from the way the body faced (`facing_frame`) when taken: kept as the body
    /// turns while the player does not steer it.
    pub(super) turn: DQuat,
    /// How far before the body's axis the hands hold it by themselves (m).
    pub(super) ahead: f64,
    /// The way the body faces (horizontal), and how fast it turns (rad/s, about up).
    pub(super) facing: DVec3,
    pub(super) spin: f64,
    /// Held in one hand (the right), or two.
    pub(super) one_hand: bool,
    /// A made tool, carried in the right hand by the side, its haft forward (G1.6d).
    pub(super) tool: bool,
    /// Where the player steers it this tick, if they do.
    pub(super) input: Option<Hold>,
    /// Held still in the world (steady, H): where the grip and how turned.
    pub(super) steady: Option<(DVec3, DQuat)>,
    /// The hands' plan: where they bring the grip and how fast, the load's turn and spin.
    pub(super) plan: Plan,
    /// What the hands pulled the load with over the last step (N): the body bears it back.
    pub(super) pull: DVec3,
    /// Ticks it has been out of the hands' reach.
    pub(super) torn: u32,
    /// How far from the shoulders the point held may be (m): as far as it was taken from (a
    /// thing on the ground, the body bent to it), closing to the arms' reach as it comes in.
    pub(super) reach: f64,
    /// What clients were last told of the grip (`grip_told.rs`).
    pub(super) told: Option<super::grip_told::Told>,
}

/// Where the hands are bringing what they hold: the grip's place and speed relative to the body's
/// centre (planned in the body's frame: planned in the world, its spare-strength limit held the
/// load back as the body walked off, and 45 kg was yanked from behind and tipped it over).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Plan {
    pub(super) at: DVec3,
    pub(super) velocity: DVec3,
    pub(super) turn: DQuat,
    pub(super) spin: DVec3,
    /// Whether it is where the hands mean it (within a few centimetres): lifted.
    pub(super) there: bool,
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

    /// Avatar `by` takes body `load`'s node in hand, gripping it at `point`.
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
        // Two hands can raise at least one end of what weighs up to twice their strength (its
        // other end resting where it lies); beyond, nothing moves it.
        let mass = self.mass_of(load);
        let s = &self.world.bodies[i].structure;
        let centre = s.position[0];
        let up = character::plumb(&self.world, centre);
        let g = (self.world.planet.as_ref()).map_or(9.806_65, |p| p.acceleration(centre).length());
        if mass * g > 2.0 * carry::STRENGTH {
            return Err(format!("{mass:.0} kg: too heavy to move"));
        }
        if self.wields.remove(&player).is_some() {
            self.changed.push(by);
        }
        let facing = (point - centre)
            .reject_from(up)
            .normalize_or(up.any_orthonormal_vector());
        let l = &self.world.bodies[load].structure;
        let (c, q) = (l.position[node], l.orientation[node]);
        // A made tool in one hand at its balance, its haft forward; anything else where grabbed.
        let tool = match self.items.bodies.get(&id) {
            Some(creeer_sim::items::Item::Made(a)) => {
                let (_, centroid) = super::item_bodies::made_hull(a);
                Some(DVec3::X * a.swinging().carry - centroid)
            }
            _ => None,
        };
        let grip = tool.unwrap_or(q.conjugate() * (point - c));
        // One hand for what is light and levered little about it; else both.
        let lever = (q * grip).reject_from(up).length();
        let one_hand = tool.is_some() || (mass <= ONE_HAND && mass * g * lever <= WRIST);
        let ahead = match tool {
            Some(_) => carry::forearm(),
            None => self.ahead((load, node), point, i, facing),
        };
        self.carrying.insert(
            by,
            Carry {
                load: id,
                node,
                grip,
                // (A tool's haft, its x, the way the body faces.)
                turn: match tool {
                    Some(_) => DQuat::from_rotation_y(-std::f64::consts::FRAC_PI_2),
                    None => facing_frame(facing, up).conjugate() * q,
                },
                ahead,
                facing,
                spin: 0.0,
                one_hand,
                tool: tool.is_some(),
                input: None,
                steady: None,
                plan: Plan {
                    at: c + q * grip - centre,
                    velocity: DVec3::ZERO,
                    turn: q,
                    spin: DVec3::ZERO,
                    there: false,
                },
                pull: DVec3::ZERO,
                torn: 0,
                reach: f64::INFINITY,
                told: None,
            },
        );
        self.grip_taken(by);
        // What the hands hold passes the body's box (one box is no chest and arms: held against
        // it, a 30 kg box rubbed along it and was carried at 0.40 m/s); the hands keep it clear
        // of the body (`hands.rs`).
        self.world.let_pass(i, load);
        // (Told anew, the load is no longer predicted on its own: in the hands.) The player is
        // told what the hands hold (G1.6).
        self.changed.extend([by, id]);
        self.items.changed.insert(by);
        Ok(())
    }

    /// Lets go of the load, if any.
    pub(super) fn let_go(&mut self, by: EntityId) {
        if let Some(c) = self.carrying.remove(&by) {
            if let (Some(a), Some(b)) = (self.index(by), self.index(c.load)) {
                self.world.let_collide(a, b);
            }
            self.changed.extend([by, c.load]);
            self.items.changed.insert(by);
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
        self.carrying.get(&by).is_some_and(|c| c.plan.there)
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
