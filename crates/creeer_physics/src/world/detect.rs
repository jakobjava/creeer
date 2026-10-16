//! Finding contacts: sort and sweep over bounding balls, then exact shape pairs.

use super::shape::{Pose, RawContact, Shape};
use super::{World, narrow};
use crate::structure::NodeId;

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
}

impl World {
    fn items(&self) -> Vec<Item> {
        let mut items = Vec::new();
        for (b, body) in self.bodies.iter().enumerate() {
            let s = &body.structure;
            for &(node, shape) in &body.shapes {
                let radius = shape.bounding_radius();
                let speed = s.velocity[node].length() + s.angular_velocity[node].length() * radius;
                // Far enough to meet anything this body could reach in this step, twice over.
                let margin = 0.005 + 2.0 * speed * self.dt;
                items.push(Item {
                    body: b,
                    node,
                    shape,
                    pose: Pose {
                        position: s.position[node],
                        rotation: s.orientation[node],
                    },
                    reach: radius + margin,
                    margin,
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
        for (i, a) in items.iter().enumerate() {
            for b in &items[i + 1..] {
                if b.pose.position.x - b.reach > a.pose.position.x + a.reach {
                    break;
                }
                if a.body == b.body
                    || (asleep(a.body) && asleep(b.body))
                    || (a.pose.position - b.pose.position).length() > a.reach + b.reach
                {
                    continue;
                }
                let margin = a.margin + b.margin;
                for raw in narrow::shapes((a.pose, a.shape), (b.pose, b.shape), margin) {
                    found.push((a.body, a.node, Some((b.body, b.node)), raw));
                }
            }
            if asleep(a.body) {
                continue;
            }
            for fixed in &self.statics {
                for raw in narrow::with_static((a.pose, a.shape), fixed, a.margin) {
                    found.push((a.body, a.node, None, raw));
                }
            }
            if let Some(ground) = &self.ground {
                for raw in super::ground::contacts(&**ground, a.pose, a.shape, a.margin) {
                    found.push((a.body, a.node, None, raw));
                }
            }
        }
        found
    }
}
