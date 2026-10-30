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
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

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

/// The shapes rays are cast against, by entity: each one's reach about its pose (a ball it
/// lies within, from its kind's corners alone), and its shape, built the first time a ray
/// passes within that reach and kept while the entity is what it is (2026-10-28: a felled
/// part's hull of a few hundred corners took 0.7 ms to build, and every part's was built
/// anew at every aim, twice a frame: 6 ms a frame beside four felled trees). With the number
/// built so far.
#[derive(Debug, Default)]
pub(crate) struct AimShapes(Mutex<BTreeMap<EntityId, AimShape>>, AtomicU64);

/// An entity's reach, and its shape once built (none: one aimed past).
type AimShape = (f64, Option<Option<Shape>>);

impl Connection {
    /// The shapes aimed at built so far (each once while its entity is what it is).
    pub fn aim_shapes_built(&self) -> u64 {
        self.aim_shapes.1.load(Ordering::Relaxed)
    }
}

impl AimShapes {
    /// Entity `id` is something else now, or gone.
    pub(crate) fn forget(&mut self, id: EntityId) {
        self.0
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&id);
    }
}

/// A ball about an entity's pose that holds the shape `shape` gives it (m): never smaller than
/// that shape's own bounding radius, so a ray it keeps clear of misses the shape too.
fn reach_of(kind: &Kind) -> f64 {
    // A hull about its centroid: its corners are within twice their spread about their mean.
    let about_mean = |corners: &[DVec3]| {
        #[allow(clippy::cast_precision_loss)] // corners
        let mean = corners.iter().sum::<DVec3>() / corners.len().max(1) as f64;
        2.0 * corners
            .iter()
            .map(|c| (*c - mean).length())
            .fold(0.0, f64::max)
    };
    match kind {
        Kind::Body(shape, _) | Kind::Segment { shape, .. } => match shape {
            NetShape::Box(half) => half.length(),
            NetShape::Sphere(radius) => *radius,
            NetShape::Hull(corners) => about_mean(corners),
        },
        Kind::Character { .. } => character::torso().length(),
        Kind::Structure(_) => 0.0,
        // About the centre of mass, its corners as they are.
        Kind::Tree(t) => t.hull.iter().map(|p| p.length()).fold(0.0, f64::max),
    }
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
        // A felled tree's stem (G1.7): its hull about its centre of mass, as the server has it.
        Kind::Tree(t) => {
            let (hull, centroid) = Hull::new(&t.hull).ok()?;
            return Some(Shape::Hull(Arc::new(hull.about(-centroid))));
        }
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
        self.aim_skipping(origin, direction, length, None)
    }

    /// As [`Self::aim`], passing through `skip` too (what the hands hold, placing it).
    pub fn aim_skipping(
        &self,
        origin: DVec3,
        direction: DVec3,
        length: f64,
        skip: Option<EntityId>,
    ) -> Option<Aimed> {
        let own = self.welcome.as_ref().map(|w| w.avatar);
        let aside = |id: EntityId| Some(id) == own || Some(id) == skip;
        let predicted: Vec<EntityId> = (self.predicted.iter())
            .flat_map(|p| p.nearby().map(|(id, _)| id))
            .collect();
        let at_own = self.predicted.as_ref().and_then(|p| p.tick());
        let mut best: Option<Aimed> = None;
        let mut shapes = (self.aim_shapes.0.lock()).unwrap_or_else(PoisonError::into_inner);
        for (id, pose) in self.poses() {
            // A built structure (G2.3): its members as drawn, each a capsule round its axis.
            if let Some(Kind::Structure(frame)) = self.kinds.get(&id)
                && !aside(id)
            {
                let hit = (frame.members.iter().zip(&frame.radii))
                    .filter(|(_, r)| **r > 0.0)
                    .filter_map(|([a, b], r)| {
                        let node = |k: u32| {
                            frame
                                .nodes
                                .get(k as usize)
                                .map(|n| pose.position + pose.orientation * *n)
                        };
                        capsule_ray(origin, direction, (node(*a)?, node(*b)?), *r)
                    })
                    .fold(None, |best: Option<f64>, t| {
                        Some(best.map_or(t, |b| b.min(t)))
                    });
                if let Some(t) = hit.filter(|t| *t <= length)
                    && best.is_none_or(|b| t < b.distance)
                {
                    best = Some(Aimed {
                        body: Some(id),
                        point: origin + direction * t,
                        distance: t,
                        seen: self.drawn_at,
                    });
                }
                continue;
            }
            let Some(kind) = (!aside(id)).then(|| self.kinds.get(&id)).flatten() else {
                continue;
            };
            // Too far from the line to reach it (its reach first, its shape's own radius then).
            let along = (pose.position - origin).dot(direction);
            let off = (pose.position - origin - direction * along).length();
            let (reach, built) = shapes.entry(id).or_insert_with(|| (reach_of(kind), None));
            if off > *reach || along > length + *reach {
                continue;
            }
            let build = || {
                self.aim_shapes.1.fetch_add(1, Ordering::Relaxed);
                shape(kind)
            };
            let Some(shape) = built.get_or_insert_with(build).as_ref() else {
                continue;
            };
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

impl Connection {
    /// Strikes `aimed` with the tool in hand (G1.4e): swings the player's own avatar at once,
    /// from its own tick, and asks the server to begin the swing then (the server's word of it
    /// is for the others).
    pub fn strike(&mut self, aimed: Aimed) {
        let command = self.strike_command(aimed);
        self.command_swing(command);
    }

    /// Strikes loose thing `loose` lying on the ground (G1.6), as `strike` a body.
    pub fn strike_loose(&mut self, loose: u64) {
        let command = self.strike_loose_command(loose);
        self.command_swing(command);
    }

    /// The command striking loose thing `loose` asks the server.
    pub fn strike_loose_command(&self, loose: u64) -> creeer_net::message::Command {
        let own = self.predicted.as_ref().and_then(|p| p.tick());
        let started = own.map_or(self.drawn_at, f64::from);
        creeer_net::message::Command::StrikeLoose { loose, started }
    }

    /// Sends a swing's `command`, its swing drawn from now.
    fn command_swing(&mut self, command: creeer_net::message::Command) {
        self.command(command);
        let own = self.predicted.as_ref().and_then(|p| p.tick());
        if let (Some(w), Some(tick)) = (&self.welcome, own) {
            self.swings.insert(w.avatar, tick);
        }
    }

    /// The command striking `aimed` asks the server (the swing begun at the player's own tick).
    pub fn strike_command(&self, aimed: Aimed) -> creeer_net::message::Command {
        let own = self.predicted.as_ref().and_then(|p| p.tick());
        let started = own.map_or(self.drawn_at, f64::from);
        let target = creeer_net::message::Target {
            body: aimed.body,
            at: aimed.point,
            seen: aimed.seen,
        };
        creeer_net::message::Command::Strike { target, started }
    }

    /// How long ago character `id` began the swing it is in (s), as it is drawn: the player's
    /// own avatar at its own tick, any other a little in the past; none past its cycle.
    pub fn swinging(&self, id: EntityId) -> Option<f64> {
        let started = f64::from(*self.swings.get(&id)?);
        let own = self.welcome.as_ref().is_some_and(|w| w.avatar == id);
        let now = match self.predicted.as_ref().and_then(|p| p.tick()) {
            Some(tick) if own => f64::from(tick),
            _ => self.drawn_at,
        };
        let seconds = (now - started) / f64::from(creeer_net::channel::TICK_RATE);
        (0.0..creeer_sim::strike::CYCLE)
            .contains(&seconds)
            .then_some(seconds)
    }
}

impl Connection {
    /// The load the player's own avatar carries, drawn in its hands: where it is from the
    /// avatar as both are drawn a little in the past (`interpolated`), from the avatar as
    /// predicted (`predicted`), so it moves with the hands (G1.4f: drawn in the past behind a
    /// predicted avatar, it floated behind them).
    pub(super) fn in_own_hands(
        &self,
        poses: &mut std::collections::BTreeMap<EntityId, creeer_net::interp::Pose>,
        interpolated: Option<creeer_net::interp::Pose>,
        predicted: creeer_net::interp::Pose,
    ) {
        let own = self.welcome.as_ref().map(|w| w.avatar);
        let carries = match own.and_then(|a| self.kinds.get(&a)) {
            Some(Kind::Character { carries, .. }) => carries.map(|c| c.load),
            _ => None,
        };
        let (Some(load), Some(from)) = (carries, interpolated) else {
            return;
        };
        if let Some(pose) = poses.get_mut(&load) {
            let back = from.orientation.conjugate();
            let (at, turn) = (
                back * (pose.position - from.position),
                back * pose.orientation,
            );
            pose.position = predicted.position + predicted.orientation * at;
            pose.orientation = (predicted.orientation * turn).normalize();
        }
    }
}

impl Connection {
    /// The fixed shapes the avatar's prediction meets (G1.7: the trunks about it, as the server
    /// lays them).
    pub fn set_statics(&mut self, statics: Vec<creeer_physics::world::shape::StaticShape>) {
        if let Some(p) = self.predicted.as_mut() {
            p.set_statics(statics);
        }
    }

    /// Strikes standing tree `tree` at `height` up its stem (G1.7), as `strike` a body.
    pub fn strike_tree(&mut self, tree: u64, height: f64) {
        let command = self.strike_tree_command(tree, height);
        self.command_swing(command);
    }

    /// The command striking standing tree `tree` at `height` asks the server (the swing begun at
    /// the player's own tick).
    pub fn strike_tree_command(&self, tree: u64, height: f64) -> creeer_net::message::Command {
        let own = self.predicted.as_ref().and_then(|p| p.tick());
        let started = own.map_or(self.drawn_at, f64::from);
        creeer_net::message::Command::StrikeTree {
            tree,
            height,
            started,
        }
    }
}

/// Where the ray from `origin` along unit `direction` first meets the capsule of radius `r`
/// round the segment `(a, b)` (m along the ray), if it does: the axis's point nearest the ray,
/// the ray's point nearest that, less the half chord of the round side there.
fn capsule_ray(origin: DVec3, direction: DVec3, (a, b): (DVec3, DVec3), r: f64) -> Option<f64> {
    let axis = b - a;
    let (l2, da) = (axis.length_squared(), direction.dot(axis));
    let w = origin - a;
    let denominator = l2 - da * da;
    // The axis's parameter nearest the ray (parallel: its nearest end to the origin).
    let s = if denominator.abs() < 1e-12 * l2 {
        (w.dot(axis) / l2).clamp(0.0, 1.0)
    } else {
        ((w.dot(axis) - da * direction.dot(w)) / denominator).clamp(0.0, 1.0)
    };
    let on_axis = a + axis * s;
    let t = (on_axis - origin).dot(direction);
    let gap = (origin + direction * t - on_axis).length();
    (gap <= r && t >= 0.0).then(|| (t - (r * r - gap * gap).sqrt()).max(0.0))
}

#[cfg(test)]
mod capsule_tests {
    use super::capsule_ray;
    use creeer_math::glam::DVec3;

    #[test]
    fn a_ray_meets_a_members_round_side_and_misses_beside_it() {
        let member = (DVec3::new(-1.0, 0.0, 0.0), DVec3::new(1.0, 0.0, 0.0));
        let hit = capsule_ray(DVec3::new(0.3, 5.0, 0.0), DVec3::NEG_Y, member, 0.1);
        assert!((hit.expect("meets it") - 4.9).abs() < 1e-12);
        assert!(capsule_ray(DVec3::new(0.3, 5.0, 0.2), DVec3::NEG_Y, member, 0.1).is_none());
        assert!(capsule_ray(DVec3::new(0.3, -5.0, 0.0), DVec3::NEG_Y, member, 0.1).is_none());
    }
}
