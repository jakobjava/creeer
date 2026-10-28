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
    /// Each body's group, as the least body of it: bodies joined by the contacts `found`, by
    /// joints and by struts (union-find).
    pub(super) fn roots(&self, found: &[Found]) -> Vec<usize> {
        let mut parent: Vec<usize> = (0..self.bodies.len()).collect();
        fn root(parent: &mut [usize], mut i: usize) -> usize {
            while parent[i] != i {
                parent[i] = parent[parent[i]];
                i = parent[i];
            }
            i
        }
        let joined = (self.joints.iter()).filter_map(|j| j.b.map(|b| (j.a.0, b.0)));
        let strutted = (self.struts.iter()).filter_map(|s| s.b.map(|b| (s.a.0, b.0)));
        let touching = found
            .iter()
            .filter_map(|(a, _, b, _)| b.map(|(b, _)| (*a, b)));
        for (a, b) in touching.chain(joined).chain(strutted) {
            let (ra, rb) = (root(&mut parent, a), root(&mut parent, b));
            parent[ra.max(rb)] = ra.min(rb);
        }
        (0..self.bodies.len())
            .map(|b| root(&mut parent, b))
            .collect()
    }

    pub(super) fn pass(&mut self, found: &[Found], anchors: &[anchor::Anchor]) -> Vec<Stepped> {
        let setup = crate::profile::start();
        // Groups: bodies joined by contacts, joints and struts, of those stepped now.
        let roots = self.roots(found);
        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut group_of = vec![usize::MAX; self.bodies.len()];
        for body in
            (0..self.bodies.len()).filter(|&b| !self.bodies[b].sleep.asleep && self.in_step(b))
        {
            let r = roots[body];
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
        let left = self.left_by_group(&groups, &group_of);
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
        // Step every group, in parallel, each borrowing its own bodies (in their order, as
        // its contacts number them): moving them out and back cost more than stepping 200
        // free bodies.
        let joints = self.group_joints(&groups, &group_of);
        let dt = self.dt;
        let ground = self.ground.clone();
        let mut members: Vec<Vec<&mut Body>> =
            groups.iter().map(|g| Vec::with_capacity(g.len())).collect();
        for (b, body) in self.bodies.iter_mut().enumerate() {
            if let Some(g) = members.get_mut(group_of[b]) {
                g.push(body);
            }
        }
        let mut work: Vec<stepping::GroupWork<'_>> = groups
            .iter()
            .zip(contacts)
            .zip(left)
            .zip(members)
            .zip(joints)
            .map(
                |((((g, (c, p, measures, shapes)), left), bodies), joints)| {
                    let mut contacts = Contacts::new(c, p, measures, (ground.clone(), shapes));
                    contacts.set_left(left);
                    contacts.joints = joints;
                    (bodies, contacts, Vec::new(), g)
                },
            )
            .collect();
        setup.stop(crate::profile::Slot::PassSetup);
        work.par_iter_mut()
            .for_each(|(bodies, contacts, anchors, group)| {
                let _scope = creeer_core::profile::scope("physics: group");
                let timer = crate::profile::start();
                stepping::step(bodies, contacts, dt);
                *anchors = anchor::after_step(bodies, group, contacts);
                timer.stop(crate::profile::Slot::GroupWorld);
            });
        // The joints' constraint forces, carried to the next step.
        for (_, contacts, _, _) in &work {
            for (index, joint) in &contacts.joints.list {
                self.joints[*index].multipliers = joint.multipliers;
                self.joints[*index].anchor = joint.anchor;
                self.joints[*index].applied = joint.applied;
            }
        }
        work.into_iter()
            .map(|(_, contacts, anchors, g)| (g.clone(), contacts, anchors))
            .collect()
    }

    /// Each group's joints, their sides numbered as the group's members, with the sides'
    /// mass and inertia (for the joints' penalties).
    fn group_joints(&self, groups: &[Vec<usize>], group_of: &[usize]) -> Vec<super::joint::Joints> {
        let mut lists = vec![(Vec::new(), Vec::new()); groups.len()];
        for (index, joint) in self.joints.iter().enumerate() {
            let Some(&g) = group_of.get(joint.a.0).filter(|&&g| g != usize::MAX) else {
                continue;
            };
            let member = |body: usize| groups[g].iter().position(|&b| b == body);
            let (Some(a), b) = (member(joint.a.0), joint.b.map(|b| member(b.0))) else {
                continue;
            };
            let mut local = joint.clone();
            local.a.0 = a;
            if let (Some(side), Some(Some(m))) = (&mut local.b, b) {
                side.0 = m;
            }
            let inertia = |side: Option<(usize, NodeId)>| {
                side.map_or((f64::INFINITY, f64::INFINITY, false), |(body, node)| {
                    let s = &self.bodies[body].structure;
                    let i = s.inertia[node];
                    let rigid = s.node_count() == 1 && s.element_count() == 0;
                    (
                        s.mass[node],
                        (i.x_axis.x + i.y_axis.y + i.z_axis.z) / 3.0,
                        rigid,
                    )
                })
            };
            lists[g].0.push((index, local));
            lists[g].1.push([inertia(Some(joint.a)), inertia(joint.b)]);
        }
        let mut joints: Vec<super::joint::Joints> = (lists.into_iter())
            .map(|(list, sides)| super::joint::Joints::new(list, sides))
            .collect();
        // The struts alike (`strut.rs`), with whether each side is a rigid body.
        for strut in &self.struts {
            let Some(&g) = group_of.get(strut.a.0).filter(|&&g| g != usize::MAX) else {
                continue;
            };
            let member = |body: usize| groups[g].iter().position(|&b| b == body);
            let rigid = |side: Option<(usize, NodeId)>| {
                side.is_some_and(|(body, _)| {
                    let s = &self.bodies[body].structure;
                    s.node_count() == 1 && s.element_count() == 0
                })
            };
            let mut local = *strut;
            let Some(a) = member(strut.a.0) else { continue };
            local.a.0 = a;
            if let Some((body, node)) = strut.b {
                let Some(b) = member(body) else { continue };
                local.b = Some((b, node));
            }
            joints[g].add_strut(local, [rigid(Some(strut.a)), rigid(strut.b)]);
        }
        // The grips alike (`grip.rs`): on their body's group.
        for grip in &self.grips {
            let Some(&g) = group_of.get(grip.a.0).filter(|&&g| g != usize::MAX) else {
                continue;
            };
            let Some(a) = groups[g].iter().position(|&b| b == grip.a.0) else {
                continue;
            };
            let s = &self.bodies[grip.a.0].structure;
            let rigid = s.node_count() == 1 && s.element_count() == 0;
            let mut local = *grip;
            local.a.0 = a;
            joints[g].add_grip(local, rigid);
        }
        joints
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
        // A turning normal starts from the bodies as they are found (the step's start): the
        // energy guard measures the contacts before the step sets them up, and with no start
        // (identity) it turned each normal by its body's whole orientation (a ragdoll's foot
        // under a crate found 21 J in an edge 6 mm apart, every step, `turning.rs`).
        let orientation = |body: usize, node: NodeId| self.bodies[body].structure.orientation[node];
        let body = |(m, body, node)| Turning::Body {
            at: (m, node),
            from: orientation(body, node),
        };
        contact.turning = match raw.follows {
            Follows::A => body((member(a), a, na)),
            Follows::B => b.map_or(Turning::Frozen, |(b, nb)| body((member(b), b, nb))),
            Follows::Edges(ea, eb) => Turning::Edges {
                edges: (ea, eb),
                from: (
                    orientation(a, na),
                    b.map_or(DQuat::IDENTITY, |(b, nb)| orientation(b, nb)),
                ),
                sign: if ea.cross(eb).dot(raw.normal) < 0.0 {
                    -1.0
                } else {
                    1.0
                },
            },
            Follows::Neither | Follows::ShapeA | Follows::ShapeB => Turning::Frozen,
        };
        contact.radius_a = raw.radius_a;
        contact.radius_b = raw.radius_b;
        contact.bump = super::ground_rise::is_bump(&raw);
        contact.projected = raw.patch.as_ref().is_some_and(|v| !v.corner_on_a);
        let kept = anchors.find((a, na), local_a, b);
        if contact.rolls() || contact.projected {
            // A contact with a sphere: the anchor lies under A's surface point, less the
            // stick offset carried over from the last step (`friction.rs`). So does a face's
            // corner on B, whose point on A is where it falls on A's face, found afresh: kept
            // as B's corner, its anchor was under A's point again each step, the slip a step
            // had built was forgotten by the next, and a crate on a crate crept down a ramp
            // well within its friction (4.5 mm in 2 s, G2.0).
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
