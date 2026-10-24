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
use std::collections::HashMap;

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
    pub(super) a: (usize, NodeId),
    pub(super) local_a: DVec3,
    pub(super) b: Option<(usize, NodeId)>,
    pub(super) kept: Kept,
    /// A bump of the ground's inset (`ground_rise::bump`), kept for the next step's detection.
    pub(super) bump: Option<f64>,
}

/// The bodies (and nodes) an anchor joins: A's, and B's or none for fixed geometry.
type Pair = ((usize, NodeId), Option<(usize, NodeId)>);

/// Last step's anchors by the pair they join. Scanning all of them for every contact grew
/// as the square of the contacts (0.15 ms a tick for 400, F5).
pub struct Index<'a> {
    anchors: &'a [Anchor],
    by_pair: HashMap<Pair, Vec<usize>>,
}

impl<'a> Index<'a> {
    /// The index of `anchors` (each pair's in their order).
    pub fn new(anchors: &'a [Anchor]) -> Self {
        let mut by_pair: HashMap<Pair, Vec<usize>> = HashMap::new();
        for (i, x) in anchors.iter().enumerate() {
            by_pair.entry((x.a, x.b)).or_default().push(i);
        }
        Self { anchors, by_pair }
    }

    /// Last step's anchor for this contact, if it had one.
    pub fn find(
        &self,
        a: (usize, NodeId),
        local_a: DVec3,
        b: Option<(usize, NodeId)>,
    ) -> Option<Kept> {
        find(
            self.by_pair.get(&(a, b))?.iter().map(|&i| &self.anchors[i]),
            local_a,
        )
    }
}

/// The bumps of the ground the last step had, by the body and node they are on: each one's
/// point (in the node's frame) and inset (`ground_rise::bump`).
pub fn bumps(anchors: &[Anchor]) -> HashMap<(usize, NodeId), Vec<(DVec3, f64)>> {
    let mut out: HashMap<(usize, NodeId), Vec<(DVec3, f64)>> = HashMap::new();
    for x in anchors.iter().filter(|x| x.b.is_none()) {
        if let Some(inset) = x.bump {
            out.entry(x.a).or_default().push((x.local_a, inset));
        }
    }
    out
}

/// A bump's point found again within this distance (m, in its node's frame) is the same
/// bump: its points lie where a face's or an edge's grid puts them, centimetres apart, and
/// come back to within rounding.
pub const SAME_BUMP: f64 = 1.0e-4;

/// The anchor among `candidates` (of the contact's pair) nearest its point, if one is near.
fn find<'a>(candidates: impl Iterator<Item = &'a Anchor>, local_a: DVec3) -> Option<Kept> {
    candidates
        .map(|x| (x, (x.local_a - local_a).length()))
        .filter(|(_, d)| *d < SAME_POINT)
        .min_by(|p, q| p.1.total_cmp(&q.1))
        .map(|(x, _)| x.kept)
}

/// The anchors after a group's step (moved where the contacts slid). `group` lists the
/// global indices of the group's bodies.
pub fn after_step(bodies: &[&mut Body], group: &[usize], contacts: &Contacts) -> Vec<Anchor> {
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
            let kept = if c.rolls() || c.projected {
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
                bump: c.bump.then_some(c.offset),
            })
        })
        .collect()
}

impl Anchor {
    /// The anchor with its bodies renumbered by `map` (old index to new), or none if one of
    /// them was removed.
    pub fn renumbered(&self, map: &[Option<usize>]) -> Option<Self> {
        let a = (map.get(self.a.0).copied().flatten()?, self.a.1);
        let b = match self.b {
            Some((b, node)) => Some((map.get(b).copied().flatten()?, node)),
            None => None,
        };
        Some(Self { a, b, ..*self })
    }
}
