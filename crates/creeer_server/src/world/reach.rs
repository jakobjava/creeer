//! What a player reaches for (G1.4d, ADR-0027 decision 4), judged as they saw it: a point on a
//! body, as that body was at the tick the client drew it (`history.rs`), or on the ground;
//! within reach of the hands (`creeer_sim::reach`) and in the eye's sight past the ground (a
//! camera behind the character sees over a hill the character cannot: the camera's ray says
//! what is meant, the character's whether it can get there, as third-person games trace from
//! the camera and then from the character).

use super::history::{KEPT, Nodes};
use super::*;
use creeer_net::predict::MOST_BUFFER;
use creeer_sim::reach::{self, HISTORY, REACH};

/// How far from a body's surface, or the ground's, a point seen on it may lie (m): the
/// snapshots' quantised states and the drawing between them are a few millimetres off.
const ON: f64 = 0.05;
/// How far ahead of the server's a player's own avatar may be (s): the client draws it as
/// predicted, its inputs waiting in the server's jitter buffer, 100 ms at most.
#[allow(clippy::cast_precision_loss)] // a few ticks
const AHEAD: f64 = MOST_BUFFER as f64 * DT;

/// What a player reached.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reached {
    /// The body and its node (none: the ground).
    pub body: Option<(usize, usize)>,
    /// Where the point reached is now (the body moves on with it).
    pub point: DVec3,
}

impl ServerWorld {
    /// Keeps where the bodies near the players are this tick, for `reached`.
    pub(super) fn remember(&mut self) {
        let avatars: Vec<DVec3> = (self.players.iter().enumerate())
            .filter(|(_, p)| p.is_some())
            .map(|(i, _)| self.world.bodies[i].structure.position[0])
            .collect();
        let mut near: Vec<(EntityId, Nodes)> = Vec::new();
        if !avatars.is_empty() {
            for (body, id) in self.world.bodies.iter().zip(&self.ids) {
                let s = &body.structure;
                let close = (s.position.iter()).any(|p| {
                    avatars
                        .iter()
                        .any(|a| (*p - *a).length_squared() <= KEPT * KEPT)
                });
                if close {
                    let nodes = s
                        .position
                        .iter()
                        .copied()
                        .zip(s.orientation.iter().copied());
                    near.push((*id, nodes.collect()));
                }
            }
            near.sort_unstable_by_key(|b| b.0);
        }
        self.history.record(self.tick, near, u64::from(HISTORY));
    }

    /// What avatar `by` reaches at `at` (the world's frame) on `body` (none: the ground), as its
    /// player saw it at tick `seen` (the world's ticks, fractional: a client draws between
    /// them; a tick still to come is taken as now).
    ///
    /// # Errors
    /// Why it cannot: no such avatar or body, seen too long ago or not near it then, the point
    /// not on what it names, beyond reach, or behind the ground.
    pub fn reached(
        &self,
        by: EntityId,
        body: Option<EntityId>,
        at: DVec3,
        seen: f64,
    ) -> Result<Reached, String> {
        if !at.is_finite() || !seen.is_finite() {
            return Err("a point that is not finite".into());
        }
        let avatar = self.index(by).ok_or("no such avatar")?;
        #[allow(clippy::cast_precision_loss)] // ticks within 2⁵³
        let now = self.tick as f64;
        // Seen ahead (a body predicted with the avatar is drawn at the avatar's tick, its inputs
        // still waiting here): judged as it is now, by as far as it can have gone since, for at
        // most as long as the inputs wait.
        #[allow(clippy::cast_precision_loss)]
        let ahead = (seen - now).clamp(0.0, MOST_BUFFER as f64);
        let seen = seen.min(now);
        if now - seen > f64::from(HISTORY) {
            return Err(format!("seen {:.0} ticks ago", now - seen));
        }
        let reached = match body {
            None => {
                if self.terrain.density(at).abs() > ON {
                    return Err("not on the ground".into());
                }
                Reached {
                    body: None,
                    point: at,
                }
            }
            Some(id) => self.on_body(id, at, seen, ahead)?,
        };
        // Within reach of the hands, the avatar perhaps a little ahead of the server's.
        let s = &self.world.bodies[avatar].structure;
        let (centre, up) = (s.position[0], character::plumb(&self.world, s.position[0]));
        let slack = s.velocity[0].length() * AHEAD;
        let far = (at - reach::hands(centre, up)).length();
        if far > REACH + slack {
            return Err(format!(
                "{far:.2} m from the hands, beyond reach ({REACH} m)"
            ));
        }
        // In sight past the ground.
        let eye = reach::eye(centre, up);
        let to = at - eye;
        if let Some(hit) = self.world.ground_ray(eye, to, to.length() - ON) {
            return Err(format!(
                "behind the ground, {:.2} m from the eye",
                hit.distance
            ));
        }
        Ok(reached)
    }

    /// The point `at` on entity `id` as seen at tick `seen`, or seen `ahead` ticks after it (the
    /// newest the server has): which node's shape it lies on then, and where it is now.
    fn on_body(&self, id: EntityId, at: DVec3, seen: f64, ahead: f64) -> Result<Reached, String> {
        let index = self.index(id).ok_or("no such body")?;
        let then = self.then(id, seen).ok_or("not near a player when seen")?;
        let body = &self.world.bodies[index];
        let (node, shape) = (body.shapes.iter())
            .min_by(|a, b| {
                let d = |(n, sh): &(usize, creeer_physics::world::shape::Shape)| {
                    then.get(*n)
                        .map_or(f64::INFINITY, |(c, q)| sh.distance(*c, *q, at))
                };
                d(a).total_cmp(&d(b))
            })
            .ok_or("a body with no shape")?;
        let (c, q) = *then.get(*node).ok_or("a node not kept")?;
        let s = &body.structure;
        let gone = (s.velocity[*node].length()
            + s.angular_velocity[*node].length() * shape.bounding_radius())
            * ahead
            * DT;
        if shape.distance(c, q, at).abs() > ON + gone {
            return Err("not on the body as it was".into());
        }
        let local = q.conjugate() * (at - c);
        Ok(Reached {
            body: Some((index, *node)),
            point: s.position[*node] + s.orientation[*node] * local,
        })
    }
}

impl ServerWorld {
    /// Entity `id`'s nodes at tick `seen` (fractional): between the two ticks kept about it.
    fn then(&self, id: EntityId, seen: f64) -> Option<Nodes> {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // within the history
        let before = seen.floor().max(0.0) as u64;
        let a = self.history.at(before, id)?;
        #[allow(clippy::cast_precision_loss)]
        let share = seen - before as f64;
        if share <= 0.0 {
            return Some(a.clone());
        }
        let b = self.history.at(before + 1, id)?;
        Some(
            (a.iter().zip(b))
                .map(|((p, q), (p2, q2))| (p.lerp(*p2, share), q.slerp(*q2, share)))
                .collect(),
        )
    }
}
