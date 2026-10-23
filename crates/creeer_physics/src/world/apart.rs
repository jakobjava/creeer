//! Structures falling apart (G2.5, ADR-0033): a structure whose broken members or joints leave
//! pieces no longer held together becomes one body per piece, each as it is (moving, bent,
//! loaded), so the pieces fall, collide with each other and rest as bodies do (one body's shapes
//! never collide with each other).

use super::{Body, World};
use crate::structure::NodeId;

/// One body that came apart: its index before, and each piece's body index with the elements
/// it kept (their indices in the body before), the first piece in the body's own place.
pub type Apart = (usize, Vec<(usize, Vec<usize>)>);

impl World {
    /// Splits every structure that has come apart into one body per piece (`Structure::pieces`),
    /// and returns what came apart. Pieces that are only nodes held fast with nothing on them (a
    /// pulled post's ground) are dropped. What the world kept of the last step for those bodies
    /// is renumbered (joints) or dropped (friction anchors, contacts: found again next step).
    pub fn fall_apart(&mut self) -> Vec<Apart> {
        let mut out = Vec::new();
        for i in 0..self.bodies.len() {
            let s = &self.bodies[i].structure;
            if s.element_count() == 0 || s.elements_broken() == 0 {
                continue;
            }
            let pieces = s.pieces();
            if pieces.len() < 2 {
                continue;
            }
            let old = self.bodies[i].clone();
            let mut moved: Vec<(NodeId, usize, NodeId)> = Vec::new();
            let mut kept_pieces = Vec::new();
            for nodes in &pieces {
                let (structure, kept) = old.structure.piece(nodes);
                let fixed = (0..structure.node_count()).all(|n| structure.held_fast(n));
                if structure.element_count() == 0 && fixed {
                    continue;
                }
                let local = |n: NodeId| nodes.binary_search(&n).ok();
                let shapes = (old.shapes.iter())
                    .filter_map(|(n, s)| local(*n).map(|m| (m, s.clone())))
                    .collect();
                let mut body = Body::new(structure, shapes, self.dt);
                body.friction = old.friction;
                body.restitution = old.restitution;
                body.surface = old.surface;
                let at = if kept_pieces.is_empty() {
                    self.bodies[i] = body;
                    i
                } else {
                    self.add(body)
                };
                for (k, &n) in nodes.iter().enumerate() {
                    moved.push((n, at, k));
                }
                kept_pieces.push((at, kept));
            }
            self.forget_nodes_of(i, &moved);
            out.push((i, kept_pieces));
        }
        out
    }

    /// Body `i`'s nodes have moved (old node, new body, new node): its joints follow them, its
    /// friction anchors and the last step's contacts and energies are dropped.
    fn forget_nodes_of(&mut self, i: usize, moved: &[(NodeId, usize, NodeId)]) {
        let to = |n: NodeId| moved.iter().find(|m| m.0 == n).map(|m| (m.1, m.2));
        // A joint to a node dropped with its piece goes with it.
        self.joints.retain(|j| {
            [Some(j.a), j.b]
                .into_iter()
                .flatten()
                .all(|(b, n)| b != i || to(n).is_some())
        });
        for j in &mut self.joints {
            for side in [Some(&mut j.a), j.b.as_mut()].into_iter().flatten() {
                if side.0 == i
                    && let Some(new) = to(side.1)
                {
                    *side = new;
                }
            }
        }
        let touches = |side: Option<(usize, NodeId)>| side.is_some_and(|(b, _)| b == i);
        self.anchors.retain(|a| a.a.0 != i && !touches(a.b));
        self.left
            .energies
            .retain(|((a, b), _)| !touches(*a) && !touches(*b));
        self.contacts.clear();
        self.groups.clear();
    }
}
