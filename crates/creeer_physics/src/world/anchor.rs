//! Friction anchors kept from one step to the next (see `contact.rs`): where on body B a
//! contact point of body A sticks.

use super::Body;
use super::contact::{Contacts, Side};
use crate::structure::NodeId;
use crate::structure::group::MemberState;
use creeer_math::glam::DVec3;

/// A contact point of A within this distance (m, in A's node frame) of last step's is the
/// same contact.
const SAME_POINT: f64 = 0.02;

/// One anchor, with global body indices.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchor {
    a: (usize, NodeId),
    local_a: DVec3,
    b: Option<(usize, NodeId)>,
    /// The anchored point: in B's node frame, or in the world for fixed geometry.
    point: DVec3,
}

/// Last step's anchor for this contact, if it had one.
pub fn find(
    anchors: &[Anchor],
    a: (usize, NodeId),
    local_a: DVec3,
    b: Option<(usize, NodeId)>,
) -> Option<DVec3> {
    anchors
        .iter()
        .filter(|x| x.a == a && x.b == b)
        .map(|x| (x, (x.local_a - local_a).length()))
        .filter(|(_, d)| *d < SAME_POINT)
        .min_by(|p, q| p.1.total_cmp(&q.1))
        .map(|(x, _)| x.point)
}

/// The anchors after a group's step (moved where the contacts slid). `group` lists the
/// global indices of the group's bodies.
pub fn after_step(bodies: &[Body], group: &[usize], contacts: &Contacts) -> Vec<Anchor> {
    let states: Vec<MemberState<'_>> = bodies
        .iter()
        .map(|b| MemberState {
            position: &b.structure.position,
            orientation: &b.structure.orientation,
        })
        .collect();
    contacts
        .list
        .iter()
        .filter_map(|c| {
            let Side::Node {
                member,
                node,
                local,
            } = c.a
            else {
                return None;
            };
            let moved = contacts.moved_anchor(c, &states);
            let (b, point) = match c.anchor {
                Side::Node {
                    member: mb,
                    node: nb,
                    local: lb,
                } => {
                    let s = &bodies[mb].structure;
                    let local =
                        moved.map_or(lb, |p| s.orientation[nb].conjugate() * (p - s.position[nb]));
                    (Some((group[mb], nb)), local)
                }
                Side::Fixed(p) => (None, moved.unwrap_or(p)),
            };
            Some(Anchor {
                a: (group[member], node),
                local_a: local,
                b,
                point,
            })
        })
        .collect()
}
