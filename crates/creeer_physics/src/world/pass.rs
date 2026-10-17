//! One pass of a world step: join the awake bodies into groups by their contacts and step
//! every group (in parallel).

use super::contact::law::Hertz;
use super::contact::patch::Patch;
use super::contact::turning::Turning;
use super::contact::{Contact, Contacts, Law, Measure, Side};
use super::detect::Found;
use super::shape::{Follows, Shape};
use super::surface::{Feature, Surface, effective_radius};
use super::{Body, World, anchor, point_velocity, stepping};
use crate::structure::NodeId;
use creeer_math::glam::{DMat3, DQuat, DVec3};
use rayon::prelude::*;

/// A stepped group: its bodies (world indices), contacts and friction anchors.
pub(super) type Stepped = (Vec<usize>, Contacts, Vec<anchor::Anchor>);

impl World {
    /// Steps the awake bodies, joined into groups by the contacts `found`. `anchors`: the
    /// friction anchors at the start of the step.
    pub(super) fn pass(&mut self, found: &[Found], anchors: &[anchor::Anchor]) -> Vec<Stepped> {
        // Groups: bodies joined by contacts (union-find).
        let mut parent: Vec<usize> = (0..self.bodies.len()).collect();
        fn root(parent: &mut [usize], mut i: usize) -> usize {
            while parent[i] != i {
                parent[i] = parent[parent[i]];
                i = parent[i];
            }
            i
        }
        for (a, _, b, _) in found {
            if let Some((b, _)) = b {
                let (ra, rb) = (root(&mut parent, *a), root(&mut parent, *b));
                parent[ra.max(rb)] = ra.min(rb);
            }
        }
        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut group_of = vec![usize::MAX; self.bodies.len()];
        for body in (0..self.bodies.len()).filter(|&b| !self.bodies[b].sleep.asleep) {
            let r = root(&mut parent, body);
            if group_of[r] == usize::MAX {
                group_of[r] = groups.len();
                groups.push(Vec::new());
            }
            group_of[body] = group_of[r];
            groups[group_of[r]].push(body);
        }
        #[allow(clippy::type_complexity)] // one group's parts, built side by side
        let mut contacts: Vec<(Vec<Contact>, Vec<Patch>, Vec<Measure>, Vec<Shape>)> =
            vec![(Vec::new(), Vec::new(), Vec::new(), Vec::new()); groups.len()];
        let anchors = anchor::Index::new(anchors);
        let mut corners = [DVec3::ZERO; 4];
        for (a, na, b, raw) in found {
            let (a, na, b) = (*a, *na, *b);
            let (list, patches, measures, shapes) = &mut contacts[group_of[a]];
            // A ball against a box or a hull: measured to its closest point at every evaluation.
            let shape_of = |body: usize, node: NodeId| {
                self.bodies[body]
                    .shapes
                    .iter()
                    .find(|(n, _)| *n == node)
                    .map(|(_, s)| s.clone())
            };
            let closest = match raw.follows {
                Follows::ShapeA => shape_of(a, na).map(|s| (s, true)),
                Follows::ShapeB => b.and_then(|(b, nb)| shape_of(b, nb)).map(|s| (s, false)),
                _ => None,
            };
            measures.push(match (closest, raw.bend) {
                (Some((shape, on_a)), _) => {
                    shapes.push(shape);
                    Measure::Closest {
                        shape: shapes.len() - 1,
                        on_a,
                    }
                }
                // A round body on the ground: measured against the ground itself, from its centre
                // to its closest ground point at every evaluation. A face's corner, and the points it
                // presses with over curved ground (`ground_rise.rs`): against the ground under it to
                // second order (measured afresh at every evaluation, a resting crate's corners met
                // the voxel terrain's kinks and it never came to rest, F5).
                (None, Some(_)) if matches!(raw.features[0], Feature::Round(_)) => Measure::Ground,
                (None, Some(s)) => {
                    path_bend(s, raw.offset()).map_or(Measure::Frozen, Measure::Curved)
                }
                (None, None) => Measure::Frozen,
            });
            let (surface_a, surface_b) = (self.bodies[a].surface, self.surface_of(b));
            let modulus = surface_a.contact_modulus(&surface_b);
            let law = match &raw.patch {
                None => {
                    let radius = effective_radius(
                        (raw.features[0], surface_a.rounding),
                        (raw.features[1], surface_b.rounding),
                    );
                    Law::Hertz(Hertz::new(modulus, radius))
                }
                // A face's four corners come in a row; the patch is made with the last.
                Some(v) => {
                    let corner = if v.corner_on_a {
                        raw.point_a
                    } else {
                        raw.point_b
                    };
                    corners[usize::from(v.index)] = corner;
                    let last = v.index == 3;
                    if last {
                        let first = list.len() - 3;
                        // On the ground, the face weighs its facing as it was found turned.
                        let normal = v.facing.unwrap_or(raw.normal);
                        patches.push(Patch::new(first, corners, &v.domain, normal, modulus));
                    }
                    let patch = patches.len() - usize::from(last);
                    Law::Patch {
                        patch,
                        vertex: usize::from(v.index),
                    }
                }
            };
            list.push(self.contact(&groups, &group_of, &anchors, (a, na, b, raw.clone()), law));
        }
        // Step every group, in parallel. Bodies are moved out and back so that each group
        // owns its members.
        let mut slots: Vec<Option<Body>> = self.bodies.drain(..).map(Some).collect();
        let dt = self.dt;
        let ground = self.ground.clone();
        let mut work: Vec<stepping::GroupWork<'_>> = groups
            .iter()
            .zip(contacts)
            .map(|(g, (c, p, measures, shapes))| {
                (
                    g.iter().map(|&b| slots[b].take().expect("once")).collect(),
                    Contacts::new(c, p, measures, (ground.clone(), shapes)),
                    Vec::new(),
                    g,
                )
            })
            .collect();
        work.par_iter_mut()
            .for_each(|(bodies, contacts, anchors, group)| {
                stepping::step(bodies, contacts, dt);
                *anchors = anchor::after_step(bodies, group, contacts);
            });
        let mut stepped = Vec::with_capacity(groups.len());
        for ((bodies, contacts, anchors, _), g) in work.into_iter().zip(&groups) {
            for (body, &index) in bodies.into_iter().zip(g) {
                slots[index] = Some(body);
            }
            stepped.push((g.clone(), contacts, anchors));
        }
        self.bodies = slots.into_iter().map(|b| b.expect("returned")).collect();
        stepped
    }

    /// The surface of body `b`, or of the fixed world.
    fn surface_of(&self, b: Option<(usize, NodeId)>) -> Surface {
        b.map_or(self.static_surface, |(b, _)| self.bodies[b].surface)
    }

    /// The contact for a found pair, its sides indexed within the group.
    fn contact(
        &self,
        groups: &[Vec<usize>],
        group_of: &[usize],
        anchors: &anchor::Index<'_>,
        (a, na, b, raw): Found,
        law: Law,
    ) -> Contact {
        let member = |body: usize| {
            groups[group_of[body]]
                .iter()
                .position(|&g| g == body)
                .expect("in its group")
        };
        let side = |body: usize, node: NodeId, point: DVec3| {
            let s = &self.bodies[body].structure;
            Side::Node {
                member: member(body),
                node,
                local: s.orientation[node].conjugate() * (point - s.position[node]),
            }
        };
        let body_a = &self.bodies[a];
        let (side_b, friction, restitution, velocity_b) = match b {
            Some((b, nb)) => {
                let body_b = &self.bodies[b];
                (
                    side(b, nb, raw.point_b),
                    body_b.friction,
                    body_b.restitution,
                    point_velocity(&body_b.structure, nb, raw.point_b),
                )
            }
            None => (
                Side::Fixed(raw.point_b),
                self.static_friction,
                self.static_restitution,
                DVec3::ZERO,
            ),
        };
        let mut contact = Contact::new(
            side(a, na, raw.point_a),
            side_b,
            raw.normal,
            raw.offset(),
            law,
            (body_a.friction * friction).sqrt(),
        );
        let local_a = body_a.structure.orientation[na].conjugate()
            * (raw.point_a - body_a.structure.position[na]);
        let body = |at| Turning::Body {
            at,
            from: DQuat::IDENTITY,
        };
        contact.turning = match raw.follows {
            Follows::A => body((member(a), na)),
            Follows::B => b.map_or(Turning::Frozen, |(b, nb)| body((member(b), nb))),
            Follows::Edges(ea, eb) => Turning::Edges {
                edges: (ea, eb),
                from: (DQuat::IDENTITY, DQuat::IDENTITY),
                sign: 1.0,
            },
            Follows::Neither | Follows::ShapeA | Follows::ShapeB => Turning::Frozen,
        };
        contact.radius_a = raw.radius_a;
        contact.radius_b = raw.radius_b;
        let kept = anchors.find((a, na), local_a, b);
        if contact.rolls() {
            // A contact with a sphere: the anchor lies under A's surface point, less the
            // stick offset carried over from the last step (`friction.rs`).
            let slip = match kept {
                Some(anchor::Kept::Slip(u)) => u,
                _ => DVec3::ZERO,
            };
            let at = raw.point_a - raw.normal * raw.radius_a - slip;
            contact.anchor = match b {
                Some((bb, nb)) => side(bb, nb, at),
                None => Side::Fixed(at),
            };
        } else if let Some(anchor::Kept::Point(point)) = kept {
            contact.anchor = match b {
                Some((bb, nb)) => Side::Node {
                    member: member(bb),
                    node: nb,
                    local: point,
                },
                None => Side::Fixed(point),
            };
        }
        let mass_of = |body: usize, node: NodeId| self.bodies[body].structure.masses()[node];
        let inverse = 1.0 / mass_of(a, na) + b.map_or(0.0, |(b, nb)| 1.0 / mass_of(b, nb));
        contact.mass = 1.0 / inverse;
        contact.restitution = body_a.restitution.min(restitution);
        let relative = point_velocity(&body_a.structure, na, raw.point_a) - velocity_b;
        contact.approach = raw.normal.dot(relative);
        contact.sliding = (relative - raw.normal * contact.approach).length();
        contact
    }
}

/// The shape operator of the path of a point `offset` above a surface with shape operator `s`
/// (a ball's centre over the ground): S·(I − offset·S)⁻¹, the parallel surface's. `None`
/// where the ball is about as curved as the ground (it cannot rest in a hollow its own size).
fn path_bend(s: DMat3, offset: f64) -> Option<DMat3> {
    let shrink = DMat3::IDENTITY - s * offset;
    (shrink.determinant() > 0.1).then(|| s * shrink.inverse())
}
