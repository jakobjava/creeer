//! One pass of a world step: join the awake bodies into groups by their contacts and step
//! every group (in parallel).

use super::contact::{Contact, Contacts, Side};
use super::detect::Found;
use super::{Body, World, anchor, point_velocity, stepping};
use crate::structure::NodeId;
use creeer_math::glam::DVec3;
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
        let mut contacts: Vec<Vec<Contact>> = vec![Vec::new(); groups.len()];
        for &(a, na, b, raw) in found {
            let contact = self.contact(&groups, &group_of, anchors, (a, na, b, raw));
            contacts[group_of[a]].push(contact);
        }
        // Step every group, in parallel. Bodies are moved out and back so that each group
        // owns its members.
        let mut slots: Vec<Option<Body>> = self.bodies.drain(..).map(Some).collect();
        let dt = self.dt;
        let mut work: Vec<stepping::GroupWork<'_>> = groups
            .iter()
            .zip(contacts)
            .map(|(g, c)| {
                (
                    g.iter().map(|&b| slots[b].take().expect("once")).collect(),
                    Contacts::new(c),
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

    /// The contact for a found pair, its sides indexed within the group.
    fn contact(
        &self,
        groups: &[Vec<usize>],
        group_of: &[usize],
        anchors: &[anchor::Anchor],
        (a, na, b, raw): Found,
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
        let k = self.contact_stiffness;
        let mut contact = Contact::new(
            side(a, na, raw.point_a),
            side_b,
            raw.normal,
            raw.offset,
            k,
            // Overdamps the contact's own (unresolvable) vibration within the step.
            k * self.dt,
            (body_a.friction * friction).sqrt(),
        );
        let local_a = body_a.structure.orientation[na].conjugate()
            * (raw.point_a - body_a.structure.position[na]);
        if let Some(point) = anchor::find(anchors, (a, na), local_a, b) {
            contact.anchor = match b {
                Some((bb, nb)) => Side::Node {
                    member: member(bb),
                    node: nb,
                    local: point,
                },
                None => Side::Fixed(point),
            };
        }
        contact.restitution = body_a.restitution.min(restitution);
        let relative = point_velocity(&body_a.structure, na, raw.point_a) - velocity_b;
        contact.approach = raw.normal.dot(relative);
        contact.sliding = (relative - raw.normal * contact.approach).length();
        contact
    }
}
