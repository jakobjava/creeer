//! What clients are told of the entities: what each is when it comes into view, its state in
//! snapshots (its node 0's, quantised), and how a structure's shape changed for good.
//!
//! A structure is drawn from its node 0's state and its nodes' places relative to it (in
//! node 0's frame). Those are sent when it comes into view, and again for the nodes that
//! moved by more than [`SHAPE_TOLERANCE`] since they were last told, at most every
//! [`SHAPE_EVERY`] ticks, to everyone who has it: a frame bent in a crash is drawn bent,
//! while a frame swaying by a millimetre is drawn rigid. Every client is told the same
//! shape (the one [`Shapes`] keeps), so the changes fit what each has.

use crate::world::looks::Drawn;
use creeer_math::glam::DVec3;
use creeer_net::entity::{EntityId, Frame, Kind, Matter, Shape as NetShape};
use creeer_net::message::Plastic;
use creeer_net::quant::{BodyState, POSITION_SCALE, Quantised};
use creeer_physics::structure::Structure;
use creeer_physics::world::Body;
use creeer_physics::world::shape::Shape;
use std::collections::{HashMap, HashSet};

/// How far a node of a structure moves from where clients were told before they are told
/// again (m).
pub const SHAPE_TOLERANCE: f64 = 0.01;
/// Ticks between two shape changes of one structure at least.
pub const SHAPE_EVERY: u64 = 6;

/// What a client draws a body as (as `drawn`; `joined`: a joint's side).
pub fn kind(body: &Body, drawn: &Drawn, joined: bool) -> Kind {
    match drawn {
        Drawn::Person {
            player,
            look,
            fallen,
        } => {
            return Kind::Character {
                player: player.clone(),
                look: look.clone(),
                fallen: *fallen,
            };
        }
        Drawn::Segment { person, index } => {
            if let [(0, shape)] = body.shapes.as_slice() {
                return Kind::Segment {
                    person: *person,
                    index: *index,
                    shape: net_shape(shape),
                };
            }
        }
        Drawn::Thing => {}
    }
    match body.shapes.as_slice() {
        [(0, shape)] if body.structure.element_count() == 0 => {
            Kind::Body(net_shape(shape), matter(body, joined))
        }
        _ => Kind::Structure(frame(&body.structure, &offsets(&body.structure))),
    }
}

/// A shape as clients are told it.
fn net_shape(shape: &Shape) -> NetShape {
    match shape {
        Shape::Box { half } => NetShape::Box(*half),
        Shape::Sphere { radius } => NetShape::Sphere(*radius),
        Shape::Hull(hull) => NetShape::Hull(hull.vertices().to_vec()),
    }
}

/// What a rigid body is made of, for a client's prediction (ADR-0025): a lone rigid body may
/// be predicted; a joint's side (a ragdoll's) or a held one (a character's) is the server's
/// alone.
fn matter(body: &Body, joined: bool) -> Matter {
    let s = body.surface;
    Matter {
        mass: body.structure.total_mass(),
        friction: body.friction,
        restitution: body.restitution,
        surface: [s.youngs_modulus, s.poisson_ratio, s.rounding],
        predicted: !joined && !body.held,
    }
}

/// A body's state as snapshots send it.
pub fn state(body: &Body) -> BodyState {
    let s = &body.structure;
    BodyState::new(
        s.position[0],
        s.orientation[0],
        (s.velocity[0], s.angular_velocity[0]),
        body.sleep.asleep,
    )
}

/// Each node's place relative to node 0, in node 0's frame, quantised.
fn offsets(s: &Structure) -> Vec<Quantised> {
    let back = s.orientation[0].conjugate();
    s.position
        .iter()
        .map(|p| Quantised::new(back * (*p - s.position[0]), POSITION_SCALE))
        .collect()
}

fn members(s: &Structure) -> (Vec<[u32; 2]>, Vec<bool>, f64) {
    let all = s.members();
    #[allow(clippy::cast_possible_truncation)] // fewer than 4 billion nodes
    let pairs = all.iter().map(|m| [m.0 as u32, m.1 as u32]).collect();
    let radius = all.iter().map(|m| m.2).fold(0.0, f64::max);
    (pairs, all.iter().map(|m| m.3).collect(), radius)
}

fn frame(s: &Structure, nodes: &[Quantised]) -> Frame {
    let (members, _, radius) = members(s);
    Frame {
        nodes: nodes.iter().map(|q| q.value(POSITION_SCALE)).collect(),
        members,
        radius,
    }
}

/// What clients were told of a structure's shape.
#[derive(Clone, Debug)]
struct Told {
    nodes: Vec<Quantised>,
    members: Vec<[u32; 2]>,
    broken: Vec<bool>,
    at: u64,
}

/// The shapes of the structures as clients were told them.
#[derive(Debug, Default)]
pub struct Shapes {
    told: HashMap<EntityId, Told>,
}

impl Shapes {
    /// What a client is told when structure `id` (`body`) comes into view: its shape as
    /// every client was told it (the changes since come to all alike).
    pub fn kind(
        &mut self,
        id: EntityId,
        body: &Body,
        (drawn, joined): (&Drawn, bool),
        tick: u64,
    ) -> Kind {
        let s = &body.structure;
        if *drawn != Drawn::Thing || s.element_count() == 0 {
            return kind(body, drawn, joined);
        }
        let told = self.told.entry(id).or_insert_with(|| {
            let (members, broken, _) = members(s);
            Told {
                nodes: offsets(s),
                members,
                broken,
                at: tick,
            }
        });
        let mut f = frame(s, &told.nodes);
        f.members.clone_from(&told.members);
        Kind::Structure(f)
    }

    /// The shape changes since clients were last told, of every structure they were told of
    /// in `bodies` (by id), updating what they are told.
    pub fn changes<'a>(
        &mut self,
        bodies: impl Iterator<Item = (EntityId, &'a Body)>,
        tick: u64,
    ) -> Vec<(EntityId, Plastic)> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for (id, body) in bodies {
            let Some(told) = self.told.get_mut(&id) else {
                continue;
            };
            seen.insert(id);
            // (Asleep ones too: one may have changed since it was last told and then slept.)
            if tick < told.at + SHAPE_EVERY {
                continue;
            }
            let s = &body.structure;
            let now = offsets(s);
            let (members, broken, _) = members(s);
            let tolerance = SHAPE_TOLERANCE * POSITION_SCALE;
            #[allow(clippy::cast_precision_loss)] // steps of a millimetre
            let moved = |a: &Quantised, b: &Quantised| {
                let d = DVec3::from_array(std::array::from_fn(|k| (a.0[k] - b.0[k]) as f64));
                d.length() > tolerance
            };
            #[allow(clippy::cast_possible_truncation)] // fewer than 4 billion nodes
            let nodes: Vec<(u32, Quantised)> = now
                .iter()
                .enumerate()
                .filter(|(i, q)| told.nodes.get(*i).is_none_or(|t| moved(q, t)))
                .map(|(i, q)| (i as u32, *q))
                .collect();
            let new_members = (members != told.members).then(|| members.clone());
            #[allow(clippy::cast_possible_truncation)]
            let newly_broken: Vec<u32> = broken
                .iter()
                .enumerate()
                .filter(|(i, b)| {
                    **b && (new_members.is_some() || !told.broken.get(*i).copied().unwrap_or(false))
                })
                .map(|(i, _)| i as u32)
                .collect();
            if nodes.is_empty() && new_members.is_none() && newly_broken.is_empty() {
                continue;
            }
            told.nodes.resize(now.len(), Quantised::default());
            for (i, q) in &nodes {
                told.nodes[*i as usize] = *q;
            }
            told.members = members;
            told.broken = broken;
            told.at = tick;
            out.push((
                id,
                Plastic {
                    nodes,
                    members: new_members,
                    broken: newly_broken,
                },
            ));
        }
        // Structures no longer in the world.
        self.told.retain(|id, _| seen.contains(id));
        out
    }
}

#[cfg(test)]
mod tests;
