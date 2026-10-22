//! What the player aims at (G1.4d, ADR-0027): the first thing a ray meets among the bodies as
//! the client draws them and the terrain, and the server tick it was seen at, so the server can
//! judge a reach for it as the player saw it (`world/reach.rs`). Bodies predicted with the
//! avatar are drawn at the avatar's own tick, the others a little in the past.

use super::Connection;
use creeer_math::glam::DVec3;
use creeer_net::entity::{EntityId, Kind, Shape as NetShape};
use creeer_physics::world::hull::Hull;
use creeer_physics::world::ray::ground_ray;
use creeer_physics::world::shape::Shape;
use creeer_sim::TerrainGround;
use creeer_sim::character;
use std::sync::Arc;

/// What a ray met.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aimed {
    /// The body (none: the ground).
    pub body: Option<EntityId>,
    /// Where (the world's frame), and how far along the ray (m).
    pub point: DVec3,
    pub distance: f64,
    /// The server tick it was drawn at (fractional).
    pub seen: f64,
}

/// The shape an entity is drawn with, as the physics knows shapes (none: one aimed past).
fn shape(kind: &Kind) -> Option<Shape> {
    let net = match kind {
        Kind::Body(shape, _) | Kind::Segment { shape, .. } => shape,
        Kind::Character { fallen: false, .. } => {
            return Some(Shape::Box {
                half: character::torso(),
            });
        }
        // A fallen character's pelvis is among its segments; structures' members are not
        // aimed at yet.
        Kind::Character { .. } | Kind::Structure(_) => return None,
    };
    Some(match net {
        NetShape::Box(half) => Shape::Box { half: *half },
        NetShape::Sphere(radius) => Shape::Sphere { radius: *radius },
        NetShape::Hull(corners) => Shape::Hull(Arc::new(Hull::new(corners).ok()?.0)),
    })
}

impl Connection {
    /// What the ray from `origin` along unit `direction` meets first within `length` (m): a
    /// body as drawn now (the player's own avatar left out), or the terrain.
    pub fn aim(&self, origin: DVec3, direction: DVec3, length: f64) -> Option<Aimed> {
        let own = self.welcome.as_ref().map(|w| w.avatar);
        let predicted: Vec<EntityId> = (self.predicted.iter())
            .flat_map(|p| p.nearby().map(|(id, _)| id))
            .collect();
        let at_own = self.predicted.as_ref().and_then(|p| p.tick());
        let mut best: Option<Aimed> = None;
        for (id, pose) in self.poses() {
            let Some(shape) = (Some(id) != own)
                .then(|| self.kinds.get(&id))
                .flatten()
                .and_then(shape)
            else {
                continue;
            };
            // Too far from the line to reach it.
            let along = (pose.position - origin).dot(direction);
            let off = (pose.position - origin - direction * along).length();
            if off > shape.bounding_radius() || along > length + shape.bounding_radius() {
                continue;
            }
            let Some((t, _)) = shape.ray(pose.position, pose.orientation, origin, direction) else {
                continue;
            };
            if t <= length && best.is_none_or(|b| t < b.distance) {
                let seen = match at_own {
                    Some(tick) if predicted.contains(&id) => f64::from(tick),
                    _ => self.drawn_at,
                };
                best = Some(Aimed {
                    body: Some(id),
                    point: origin + direction * t,
                    distance: t,
                    seen,
                });
            }
        }
        let within = best.map_or(length, |b| b.distance);
        if let Some(terrain) = &self.terrain
            && let Some(hit) = ground_ray(
                &TerrainGround(Arc::clone(terrain)),
                origin,
                direction,
                within,
            )
        {
            best = Some(Aimed {
                body: None,
                point: hit.point,
                distance: hit.distance,
                seen: self.drawn_at,
            });
        }
        best
    }
}
