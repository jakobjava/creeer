//! Friction anchors kept from one step to the next (see `contact.rs`): where on body B a
//! contact point of body A sticks.
//!
//! Contacts between faces, edges and corners keep the anchor as a material point of B. A
//! contact with a sphere moves over both surfaces as the sphere rolls, so it keeps its stick
//! offset instead, and its next anchor lies that far from the new contact point
//! (`friction.rs`).

use super::Body;
use super::contact::{Contacts, Side};
use crate::structure::NodeId;
use crate::structure::group::MemberState;
use creeer_math::glam::DVec3;

/// A contact point of A within this distance (m, in A's node frame) of last step's is the
/// same contact.
const SAME_POINT: f64 = 0.02;

/// What a contact keeps for the next step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kept {
    /// The anchored point: in B's node frame, or in the world for fixed geometry.
    Point(DVec3),
    /// The stick offset of a rolling contact (world): A's friction point minus the anchor.
    Slip(DVec3),
}

/// One anchor, with global body indices.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchor {
    a: (usize, NodeId),
    local_a: DVec3,
    b: Option<(usize, NodeId)>,
    kept: Kept,
}

/// Last step's anchor for this contact, if it had one.
pub fn find(
    anchors: &[Anchor],
    a: (usize, NodeId),
    local_a: DVec3,
    b: Option<(usize, NodeId)>,
) -> Option<Kept> {
    anchors
        .iter()
        .filter(|x| x.a == a && x.b == b)
        .map(|x| (x, (x.local_a - local_a).length()))
        .filter(|(_, d)| *d < SAME_POINT)
        .min_by(|p, q| p.1.total_cmp(&q.1))
        .map(|(x, _)| x.kept)
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
        .enumerate()
        .filter_map(|(i, c)| {
            let Side::Node {
                member,
                node,
                local,
            } = c.a
            else {
                return None;
            };
            let b = match c.anchor {
                Side::Node {
                    member: mb,
                    node: nb,
                    ..
                } => Some((group[mb], nb)),
                Side::Fixed(_) => None,
            };
            let kept = if c.rolls() {
                Kept::Slip(contacts.end_slip(i, &states))
            } else {
                let moved = contacts.moved_anchor(i, &states);
                Kept::Point(match c.anchor {
                    Side::Node {
                        member: mb,
                        node: nb,
                        local: lb,
                    } => {
                        let s = &bodies[mb].structure;
                        moved.map_or(lb, |p| s.orientation[nb].conjugate() * (p - s.position[nb]))
                    }
                    Side::Fixed(p) => moved.unwrap_or(p),
                })
            };
            Some(Anchor {
                a: (group[member], node),
                local_a: local,
                b,
                kept,
            })
        })
        .collect()
}
