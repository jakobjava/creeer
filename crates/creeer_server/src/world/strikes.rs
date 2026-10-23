//! Strikes (G1.4e, ADR-0027 decision 5): a player strikes what they reach (`reach.rs`) with the
//! tool in hand. The swing starts when they ask and its blow falls `strike::TO_BLOW` later, on
//! the point as it has moved with its body, if still within reach; a swing at a time, one a
//! `strike::CYCLE`. The blow is a collision of the tool's effective mass at its striking point,
//! moving at its swing's speed, with the body's effective mass there (`creeer_sim::strike`): an
//! impulse along the head's path, in the plane through the shoulders, the point and up. On the
//! ground its energy is kept for digging (G1.5).

use super::*;
use creeer_net::message::Target;
use creeer_net::predict::MOST_BUFFER;
use creeer_sim::{reach, strike};

/// What a swing will strike: the ground at a point, or a point on a body's node (in its frame).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Aim {
    Ground(DVec3),
    Body(EntityId, usize, DVec3),
}

/// A swing on its way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Swinging {
    by: EntityId,
    /// The catalogue's tool (creative), or none: what the hands hold (G1.6d).
    tool: Option<u8>,
    started: u64,
    aim: Aim,
}

/// A blow that fell: by whom, with which tool, on what (none: the ground), where, the impulse it
/// gave (N·s; none on the ground) and its energy (J).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blow {
    pub by: EntityId,
    pub tool: Option<u8>,
    pub on: Option<EntityId>,
    pub at: DVec3,
    pub impulse: DVec3,
    pub energy: f64,
}

/// Ticks in `seconds`, rounded.
fn ticks(seconds: f64) -> u64 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // seconds, not ages
    let n = (seconds / DT).round() as u64;
    n
}

impl ServerWorld {
    /// Avatar `by` strikes `target` with the tool its player holds, the swing begun at tick
    /// `started` (both the world's): its player's own tick, a few ahead of the server's while its
    /// inputs wait; taken as now if it is past, and at most the jitter buffer's ticks ahead.
    pub(super) fn strike(
        &mut self,
        by: EntityId,
        target: &Target,
        started: f64,
    ) -> Result<(), String> {
        let ready = self.ready_to_swing(by, started)?;
        let reached = self.reached(by, target.body, target.at, target.seen)?;
        let aim = match (target.body, reached.body) {
            (Some(id), Some((index, node))) => {
                let s = &self.world.bodies[index].structure;
                let local = s.orientation[node].conjugate() * (reached.point - s.position[node]);
                Aim::Body(id, node, local)
            }
            _ => Aim::Ground(reached.point),
        };
        self.begin_swing(by, ready, aim);
        Ok(())
    }

    /// Whether avatar `by` may begin a swing at its tick `started`: what it strikes with (the
    /// catalogue's tool, or none: what its hands hold) and the world's tick it begins at.
    pub(super) fn ready_to_swing(
        &self,
        by: EntityId,
        started: f64,
    ) -> Result<(Option<u8>, u64), String> {
        let i = self.index(by).ok_or("no such avatar")?;
        let player = self.players[i].clone().ok_or("only a player strikes")?;
        // The catalogue's tool in hand, or what the hands hold (G1.6d): a stone, a made axe.
        let tool = self.wields.get(&player).copied();
        let held = self.carried(by).and_then(|c| self.items.bodies.get(&c));
        if tool.is_none() && held.and_then(creeer_sim::items::Item::swinging).is_none() {
            return Err("nothing in hand to strike with".into());
        }
        if self.people.get(&by).is_some_and(people::Person::fallen) {
            return Err("down".into());
        }
        if !started.is_finite() {
            return Err("a tick that is not finite".into());
        }
        #[allow(clippy::cast_precision_loss)]
        let now = self.tick as f64;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // within a few ticks
        let started = self.tick + (started - now).round().clamp(0.0, MOST_BUFFER as f64) as u64;
        let cycle = ticks(strike::CYCLE);
        let busy = (self.swings.iter()).any(|s| s.by == by && started < s.started + cycle);
        if busy {
            return Err("still swinging".into());
        }
        Ok((tool, started))
    }

    /// Avatar `by`'s swing begins, `ready` as `ready_to_swing` found it, aimed at `aim`.
    pub(super) fn begin_swing(
        &mut self,
        by: EntityId,
        (tool, started): (Option<u8>, u64),
        aim: Aim,
    ) {
        self.started.push((by, started));
        self.swings.push(Swinging {
            by,
            tool,
            started,
            aim,
        });
    }

    /// Whether a swing on its way is aimed at body `id` (it must stay a body for the blow).
    pub(super) fn aimed_at(&self, id: EntityId) -> bool {
        (self.swings.iter()).any(|s| matches!(s.aim, Aim::Body(a, ..) if a == id))
    }

    /// The swings started since last asked: who, and the world's tick.
    pub fn take_swings_started(&mut self) -> Vec<(EntityId, u64)> {
        std::mem::take(&mut self.started)
    }

    /// The blows that fall this tick (after the step).
    pub(super) fn blows_fall(&mut self) {
        // (Last tick's, if no one took them, are gone.)
        self.blows.clear();
        let due = ticks(strike::TO_BLOW);
        let falling: Vec<Swinging> = (self.swings.iter())
            .filter(|s| s.started + due == self.tick)
            .copied()
            .collect();
        for s in falling {
            if let Some(blow) = self.blow(&s) {
                self.blows.push(blow);
            }
        }
        let cycle = ticks(strike::CYCLE);
        self.swings.retain(|s| self.tick < s.started + cycle);
    }

    /// Swing `s`'s blow, if it lands: its point still within reach of the striker standing.
    fn blow(&mut self, s: &Swinging) -> Option<Blow> {
        let i = self.index(s.by)?;
        if self.people.get(&s.by).is_some_and(people::Person::fallen) {
            return None;
        }
        let swing = match s.tool {
            Some(t) => strike::swing(self.tools.get(usize::from(t))?),
            None => {
                let held = self.items.bodies.get(&self.carried(s.by)?)?.swinging()?;
                strike::swing_of(held.mass, held.gyration, held.carry, held.strike)
            }
        };
        let (on, at) = match s.aim {
            Aim::Ground(at) => (None, at),
            Aim::Body(id, node, local) => {
                let b = &self.world.bodies[self.index(id)?].structure;
                (
                    Some((id, node)),
                    b.position[node] + b.orientation[node] * local,
                )
            }
        };
        let centre = self.world.bodies[i].structure.position[0];
        let up = character::plumb(&self.world, centre);
        if !reach::within(centre, up, at) {
            return None;
        }
        // The head's way: across the radius from the shoulders, forward and down, in the
        // plane of up and the point.
        let r = (at - reach::shoulders(centre, up)).normalize();
        let axis = r.cross(up).normalize_or_zero();
        let way = if axis == DVec3::ZERO {
            -up
        } else {
            axis.cross(r)
        };
        let way = if way.dot(up) > 0.0 { -way } else { way };
        let Some((id, node)) = on else {
            return Some(Blow {
                by: s.by,
                tool: s.tool,
                on: None,
                at,
                impulse: DVec3::ZERO,
                energy: swing.energy,
            });
        };
        let index = self.index(id)?;
        let body = &self.world.bodies[index];
        let mass = self.world.effective_mass(index, node, at, way);
        let j = strike::impulse(swing.speed, swing.mass, mass, body.restitution);
        self.world.apply_impulse(index, node, way * j, at);
        // What the blow breaks (G1.6d, `making.rs`): its collision's energy into the struck.
        let reduced = swing.mass * mass / (swing.mass + mass);
        self.broken_by(s.by, id, 0.5 * reduced * swing.speed * swing.speed, j);
        Some(Blow {
            by: s.by,
            tool: s.tool,
            on: Some(id),
            at,
            impulse: way * j,
            energy: swing.energy,
        })
    }

    /// The blows that fell this tick (G1.5 digs with them).
    pub fn take_blows(&mut self) -> Vec<Blow> {
        std::mem::take(&mut self.blows)
    }
}
