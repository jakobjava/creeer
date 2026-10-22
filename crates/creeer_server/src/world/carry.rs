//! Carrying and throwing (G1.4f, ADR-0027 decision 6): a player grabs what they reach (G1.4d's
//! checks) and holds it in both hands with a ball joint from their body to the point grabbed.
//! The hands bring it in to where a load is held (`creeer_sim::carry::held`: at the elbows, a
//! forearm ahead, the way the body goes) over a lift's second, and hold its turn to theirs with
//! a drive no stronger than the hands' grip. The joint's force is the load's weight and pull on
//! the body: it slows the body and, too far out or too heavy, tips it over (the body balances
//! within its strength, ADR-0023). More than the hands hold up (`carry::STRENGTH`) over a tenth
//! of a second, and the load drops. A throw lets go, the load given the speed of an overarm
//! throw of its mass (`carry::throw_speed`), the thrower its reaction.

use super::*;
use creeer_math::glam::DQuat;
use creeer_net::message::Target;
use creeer_physics::world::joint::{Drive, Joint, Kind};
use creeer_sim::carry;
use std::collections::VecDeque;

/// Over how many ticks the joint's force is taken against the hands' strength: a tenth of a
/// second (a knock is not a pull).
const OVER: usize = 6;
/// The hands' grip against a load's turn: the hands this far apart (m), each holding at most
/// the strength's half, and how far it turns before they give their most (rad).
const HANDS_APART: f64 = 0.4;
const GIVE: f64 = 0.1;
/// Below this speed the body keeps facing as it did (m/s).
const TURNING: f64 = 0.3;
/// How fast the body turns the way it goes (rad/s): as the drawn body turns
/// (`creeer_client::animate::stand`).
const TURN: f64 = 8.0;

/// A load carried.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Carry {
    load: EntityId,
    node: usize,
    /// Where the hands took it, in the body's frame (held from the world, a body speeding up
    /// shook the load against it), how long they take to lift it (s), and
    /// the ticks since.
    from: DVec3,
    lift: f64,
    since: u32,
    /// The way the body faces (horizontal); how fast it
    /// turns (rad/s, about up), and the hands' spare strength to turn it with (m/s² of the
    /// load: half of what its weight leaves).
    facing: DVec3,
    spin: f64,
    spare: f64,
    /// How far before the body's axis the hands hold it (m; `ahead`).
    ahead: f64,
    /// The joint's last forces (N).
    forces: VecDeque<f64>,
}

impl ServerWorld {
    /// Avatar `by` grabs `target` (its tick the world's), putting away the tool in hand.
    pub(super) fn grab(&mut self, by: EntityId, target: &Target) -> Result<(), String> {
        let i = self.index(by).ok_or("no such avatar")?;
        let player = self.players[i].clone().ok_or("only a player grabs")?;
        if self.people.get(&by).is_some_and(people::Person::fallen) {
            return Err("down".into());
        }
        if self.carrying.contains_key(&by) {
            return Err("hands full".into());
        }
        let id = target.body.ok_or("nothing to hold on the ground")?;
        let reached = self.reached(by, target.body, target.at, target.seen)?;
        let (load, node) = reached.body.ok_or("nothing to hold")?;
        if self.players[load].is_some() || self.carrying.values().any(|c| c.load == id) {
            return Err("not to be carried".into());
        }
        // Lifted with at most half the strength its weight leaves the hands.
        let mass = self.mass_of(load);
        let gravity = carry::STRENGTH / carry::MOST_LIFTED;
        let spare = 0.5 * (carry::STRENGTH / mass - gravity);
        if spare <= 0.0 {
            return Err(format!("{mass:.0} kg: too heavy to lift"));
        }
        if self.wields.remove(&player).is_some() {
            self.changed.push(by);
        }
        let s = &self.world.bodies[i].structure;
        let centre = s.position[0];
        let up = character::plumb(&self.world, centre);
        let flat = (reached.point - centre) - up * (reached.point - centre).dot(up);
        let facing = flat.normalize_or(up.any_orthonormal_vector());
        let axis = up.any_orthonormal_vector();
        let mut joint = Joint::new(
            &self.world,
            ((i, 0), Some((load, node))),
            (reached.point, axis, axis.cross(up).normalize()),
            Kind::Ball {
                cone: None,
                twist: None,
            },
        );
        joint.drive = Some(Drive {
            target: DQuat::IDENTITY,
            stiffness: grip() / GIVE,
            strength: grip(),
        });
        let (from, turn) = (joint.at_a, s.orientation[0]);
        let j = self.world.join(joint);
        // Each half of the way (up, then in) as fast as the spare strength lets: smoothly,
        // its most acceleration 24·d/T².
        let ahead = self.ahead(j, facing);
        let held = turn.conjugate() * (carry::held_out(centre, up, facing, ahead) - centre);
        let up_here = turn.conjugate() * up;
        let rise = (held - from).dot(up_here).abs();
        let inward = ((held - from) - up_here * (held - from).dot(up_here)).length();
        let lift = (24.0 * rise.max(inward) / spare).sqrt();
        self.carrying.insert(
            by,
            Carry {
                load: id,
                node,
                from,
                lift,
                since: 0,
                facing,
                spin: 0.0,
                spare,
                ahead,
                forces: VecDeque::new(),
            },
        );
        // (Told anew, the load is no longer predicted on its own: a joint's side.)
        self.changed.extend([by, id]);
        Ok(())
    }

    /// The joint avatar `by` holds its load with, by index.
    pub(super) fn hold_of(&self, by: EntityId) -> Option<usize> {
        let c = self.carrying.get(&by)?;
        let (a, b) = (self.index(by)?, self.index(c.load)?);
        (self.world.joints().iter()).position(|j| j.a == (a, 0) && j.b == Some((b, c.node)))
    }

    /// Lets go of the load, if any (its joint gone).
    pub(super) fn let_go(&mut self, by: EntityId) {
        if let Some(j) = self.hold_of(by) {
            self.world.remove_joint(j);
        }
        if let Some(c) = self.carrying.remove(&by) {
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
}

/// The most torque the hands hold a load's turn with (N·m).
fn grip() -> f64 {
    0.5 * carry::STRENGTH * 0.5 * HANDS_APART
}

impl ServerWorld {
    /// Before the step: each carrier's hands bring its load in and hold it the way the body
    /// faces (the way it goes, while it goes); one fallen lets go.
    pub(super) fn carry_before(&mut self) {
        let carriers: Vec<EntityId> = self.carrying.keys().copied().collect();
        for by in carriers {
            let (Some(i), Some(j)) = (self.index(by), self.hold_of(by)) else {
                self.carrying.remove(&by);
                self.changed.push(by);
                continue;
            };
            if self.people.get(&by).is_some_and(people::Person::fallen) {
                self.let_go(by);
                continue;
            }
            let s = &self.world.bodies[i].structure;
            let (centre, turn) = (s.position[0], s.orientation[0]);
            let up = character::plumb(&self.world, centre);
            let going = s.velocity[0] - up * s.velocity[0].dot(up);

            let c = self.carrying.get_mut(&by).expect("a carrier");
            // Turning the way it goes, no faster than the drawn body turns, and speeding up and
            // slowing down no faster than the hands' spare strength swings the load round:
            // as fast as it can still stop where it is to face.
            let want = if going.length() > TURNING {
                going.normalize()
            } else {
                c.facing
            };
            let side = up.dot(c.facing.cross(want)).signum();
            let angle = c.facing.angle_between(want) * side;
            let (reach, ahead) = (c.ahead, c.ahead);
            // The load swung round `reach` out: its speeding up (α·r) and its pull inward (ω²·r)
            // share the spare strength.
            let share = c.spare / (std::f64::consts::SQRT_2 * reach.max(0.1));
            let (most, fastest) = (share, share.sqrt().min(TURN));
            let aim = (2.0 * most * angle.abs()).sqrt().min(fastest) * angle.signum();
            c.spin += (aim - c.spin).clamp(-most * DT, most * DT);
            c.facing = (DQuat::from_axis_angle(up, c.spin * DT) * c.facing).normalize();
            let held = turn.conjugate() * (carry::held_out(centre, up, c.facing, ahead) - centre);
            let up_here = turn.conjugate() * up;
            // Lifted to the hands' height first, then brought in (not dragged along the ground).
            let from = c.from;
            let raised = from + up_here * (held - from).dot(up_here);
            let smooth = |x: f64| {
                let x = x.clamp(0.0, 1.0);
                x * x * (3.0 - 2.0 * x)
            };
            let x = 2.0 * f64::from(c.since) * DT / c.lift;
            let at = if x < 1.0 {
                from.lerp(raised, smooth(x))
            } else {
                raised.lerp(held, smooth(x - 1.0))
            };
            c.since = c.since.saturating_add(1);
            let joint = self.world.joint_mut(j);
            // Moved as the hands move: the joint's point and its velocity (the physics then
            // counts the work the hands do).
            joint.moving[0] = (at - joint.at_a) / DT;
            joint.at_a = at;
        }
    }

    /// After the step: a load pulling harder than the hands hold up, over a tenth of a second,
    /// drops.
    pub(super) fn carry_after(&mut self) {
        let carriers: Vec<EntityId> = self.carrying.keys().copied().collect();
        for by in carriers {
            let Some(j) = self.hold_of(by) else { continue };
            let force = self.world.joints()[j].force().length();
            let c = self.carrying.get_mut(&by).expect("a carrier");
            c.forces.push_back(force);
            if c.forces.len() > OVER {
                c.forces.pop_front();
            }
            #[allow(clippy::cast_precision_loss)] // a few
            let mean = c.forces.iter().sum::<f64>() / c.forces.len() as f64;
            if c.forces.len() == OVER && mean > carry::STRENGTH {
                self.let_go(by);
            }
        }
    }

    /// What avatar `by` carries, if anything.
    pub fn carried(&self, by: EntityId) -> Option<EntityId> {
        self.carrying.get(&by).map(|c| c.load)
    }

    /// After a save is loaded: the hands' joints let go (who carried what is not saved): any
    /// joint between an avatar and a body not its own ragdoll's.
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
    }
}

impl ServerWorld {
    /// Body `index`'s mass (kg): its nodes'.
    fn mass_of(&self, index: usize) -> f64 {
        let s = &self.world.bodies[index].structure;
        (0..s.node_count())
            .map(|n| self.world.effective_mass(index, n, s.position[n], DVec3::X))
            .sum()
    }
}

/// How far a load clears the body's front, held (m).
const CLEAR: f64 = 0.01;

impl ServerWorld {
    /// How far before the body's axis the hands hold the load of joint `j`, facing `facing`
    /// (m): a forearm, or farther if the load would not then clear the body's front (a load
    /// held inside the body burst out of it when let go).
    fn ahead(&self, j: usize, facing: DVec3) -> f64 {
        let joint = &self.world.joints()[j];
        let Some((b, n)) = joint.b else {
            return carry::forearm();
        };
        let body = &self.world.bodies[b];
        let (c, q) = (body.structure.position[n], body.structure.orientation[n]);
        let grab = c + q * joint.at_b;
        let a = &self.world.bodies[joint.a.0];
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

impl ServerWorld {
    /// Whether avatar `by`'s hands have lifted their load to where they hold it (for scripts:
    /// people lift, then walk off).
    pub fn lifted(&self, by: EntityId) -> bool {
        self.carrying
            .get(&by)
            .is_some_and(|c| f64::from(c.since) * DT >= c.lift)
    }
}
