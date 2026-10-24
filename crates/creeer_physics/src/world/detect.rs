//! Finding contacts: sort and sweep over bounding balls, then exact shape pairs.

use super::anchor::SAME_BUMP;
use super::ground::contacts_kept;
use super::shape::{Pose, RawContact, Shape, StaticShape};
use super::{World, narrow};
use crate::structure::NodeId;
use creeer_math::glam::DVec3;

/// A contact found at the start of a step: body A, node A, body and node B (or fixed
/// geometry), and its geometry.
pub(super) type Found = (usize, NodeId, Option<(usize, NodeId)>, RawContact);

/// A collision shape at its current place.
struct Item {
    body: usize,
    node: NodeId,
    shape: Shape,
    pose: Pose,
    /// Bounding radius plus the speculative margin.
    reach: f64,
    margin: f64,
    /// A ball about the shape's middle in its node's frame (`ball`), for bodies on several nodes.
    middle: (DVec3, f64),
}

/// How far apart (m) two shapes of one body lie in its rest shape at least, to meet each other
/// (`World::meet_within`): a member's prisms on fastened nodes lie a few millimetres apart.
const REST_APART: f64 = 0.01;

/// A ball about a shape's middle, in its node's frame: centre and radius.
fn ball(shape: &Shape) -> (DVec3, f64) {
    match shape {
        Shape::Hull(h) => {
            let (lo, hi) = (h.vertices().iter()).fold(
                (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
                |(lo, hi), &v| (lo.min(v), hi.max(v)),
            );
            let centre = 0.5 * (lo + hi);
            let radius = (h.vertices().iter())
                .map(|&v| (v - centre).length())
                .fold(0.0, f64::max);
            (centre, radius)
        }
        _ => (DVec3::ZERO, shape.bounding_radius()),
    }
}

impl World {
    /// Whether two shapes of one body meet each other (G2.5, ADR-0034): a structure's members
    /// on nodes apart in its rest shape. A built structure is one body, and its members folding
    /// through themselves as it collapses are stopped by the same contact laws as any other.
    /// Shapes that may touch in the rest shape never do: the halves of one member's prisms
    /// meeting at its nodes, members fastened together, the next node along a member. Each
    /// shape's ball about its own middle, measured in the rest shape (nodes' orientations are
    /// turns from it), apart by more than `REST_APART`. (About its node instead, a 1 m member's
    /// half-prism reached 0.5 m, and a frame's arms 40° apart passed through each other.)
    fn meet_within(&self, a: &Item, b: &Item) -> bool {
        if a.node == b.node {
            return false;
        }
        let rest = &self.bodies[a.body].structure.rest;
        let ((ca, ra), (cb, rb)) = (a.middle, b.middle);
        ((rest[a.node] + ca) - (rest[b.node] + cb)).length() > ra + rb + REST_APART
    }

    fn items(&self) -> Vec<Item> {
        let mut items = Vec::new();
        for (b, body) in self.bodies.iter().enumerate() {
            // (A step restricted to some bodies meets no others: its groups were found
            // closed against them at the step's start, `turn.rs`.)
            if !self.in_step(b) {
                continue;
            }
            let s = &body.structure;
            // Shapes of a body on several nodes may meet each other (`meet_within`).
            let many = s.node_count() > 1;
            for (node, shape) in &body.shapes {
                let node = *node;
                let radius = shape.bounding_radius();
                let speed = s.velocity[node].length() + s.angular_velocity[node].length() * radius;
                // Far enough to meet anything this body could reach in this step, twice over.
                let margin = 0.005 + 2.0 * speed * self.dt;
                items.push(Item {
                    body: b,
                    node,
                    shape: shape.clone(),
                    pose: Pose {
                        position: s.position[node],
                        rotation: s.orientation[node],
                    },
                    reach: radius + margin,
                    margin,
                    middle: if many {
                        ball(shape)
                    } else {
                        (DVec3::ZERO, radius)
                    },
                });
            }
        }
        items
    }

    /// Contacts at the start of the step, as (body A, node A, body and node B or fixed,
    /// geometry).
    pub(super) fn detect(&self) -> Vec<Found> {
        let mut items = self.items();
        // Sort and sweep along x: only shapes whose x ranges overlap are compared.
        items.sort_by(|a, b| {
            (a.pose.position.x - a.reach)
                .total_cmp(&(b.pose.position.x - b.reach))
                .then(a.body.cmp(&b.body))
                .then(a.node.cmp(&b.node))
        });
        let mut found = Vec::new();
        // Sleeping bodies need no contacts among themselves or with fixed geometry.
        let asleep = |body: usize| self.bodies[body].sleep.asleep;
        // Bodies joined do not collide with each other, unless their joint says so; nor do
        // bodies let pass.
        let unjoined = self.unjoined_pairs();
        let bumps = super::anchor::bumps(&self.anchors);
        let joined = |a: usize, b: usize| unjoined.binary_search(&(a.min(b), a.max(b))).is_ok();
        for (i, a) in items.iter().enumerate() {
            for b in &items[i + 1..] {
                if b.pose.position.x - b.reach > a.pose.position.x + a.reach {
                    break;
                }
                if (asleep(a.body) && asleep(b.body))
                    || (a.pose.position - b.pose.position).length() > a.reach + b.reach
                    || (!unjoined.is_empty() && joined(a.body, b.body))
                    || (a.body == b.body && !self.meet_within(a, b))
                {
                    continue;
                }
                let margin = a.margin + b.margin;
                for raw in narrow::shapes((a.pose, &a.shape), (b.pose, &b.shape), margin) {
                    found.push((a.body, a.node, Some((b.body, b.node)), raw));
                }
            }
            if asleep(a.body) {
                continue;
            }
            for fixed in &self.statics {
                // A box whose ball is apart from the shape's reach meets nothing (G1.7: the
                // trunks of the trees about the players).
                if let StaticShape::Box { center, half, .. } = fixed
                    && (*center - a.pose.position).length() > a.reach + half.length()
                {
                    continue;
                }
                for raw in narrow::with_static((a.pose, &a.shape), fixed, a.margin) {
                    found.push((a.body, a.node, None, raw));
                }
            }
            if let Some(ground) = &self.ground {
                // The bumps it had: kept as the last step left them (`ground_rise::bump`).
                let had = bumps.get(&(a.body, a.node)).map_or(&[][..], Vec::as_slice);
                let kept = |p: DVec3| {
                    let local = a.pose.rotation.conjugate() * (p - a.pose.position);
                    (had.iter())
                        .find(|(at, _)| (*at - local).length() < SAME_BUMP)
                        .map(|&(_, inset)| inset)
                };
                let shape = &a.shape;
                for raw in contacts_kept(&**ground, a.pose, shape, a.margin, &kept) {
                    found.push((a.body, a.node, None, raw));
                }
            }
        }
        found
    }
}
