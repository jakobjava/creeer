//! A structure falling apart (G2.5, ADR-0033): where its broken members and joints (or a cut,
//! G2.6) leave pieces no longer held together, each piece is a structure of its own, as it is
//! (moving, bent, loaded), so the pieces fall, collide and rest as bodies.

use super::{NodeId, Structure};

impl Structure {
    /// Its nodes in pieces held together by unbroken members and joints, each piece's nodes
    /// ascending, the pieces by their first node. One piece if it holds together.
    pub fn pieces(&self) -> Vec<Vec<NodeId>> {
        let n = self.node_count();
        let mut parent: Vec<usize> = (0..n).collect();
        fn root(parent: &mut [usize], mut i: usize) -> usize {
            while parent[i] != i {
                parent[i] = parent[parent[i]];
                i = parent[i];
            }
            i
        }
        for e in self.elements.iter().filter(|e| !e.broken) {
            let (a, b) = (root(&mut parent, e.a), root(&mut parent, e.b));
            if a != b {
                parent[a.max(b)] = a.min(b);
            }
        }
        let mut pieces: Vec<Vec<NodeId>> = Vec::new();
        let mut piece_of = vec![usize::MAX; n];
        for i in 0..n {
            let r = root(&mut parent, i);
            if piece_of[r] == usize::MAX {
                piece_of[r] = pieces.len();
                pieces.push(Vec::new());
            }
            pieces[piece_of[r]].push(i);
        }
        pieces
    }

    /// The piece of nodes `nodes` (ascending) as a structure of its own: those nodes as they
    /// are, and the members and joints among them (broken ones dropped). Returns it and, for
    /// each of its elements, the index it had here.
    pub fn piece(&self, nodes: &[NodeId]) -> (Self, Vec<usize>) {
        let mut new_index = vec![usize::MAX; self.node_count()];
        for (k, &i) in nodes.iter().enumerate() {
            new_index[i] = k;
        }
        fn pick<T: Copy>(v: &[T], nodes: &[NodeId]) -> Vec<T> {
            nodes.iter().map(|&i| v[i]).collect()
        }
        let mut kept = Vec::new();
        let elements = (self.elements.iter().enumerate())
            .filter(|(_, e)| {
                !e.broken && new_index[e.a] != usize::MAX && new_index[e.b] != usize::MAX
            })
            .map(|(k, e)| {
                kept.push(k);
                let mut e = *e;
                (e.a, e.b) = (new_index[e.a], new_index[e.b]);
                e
            })
            .collect();
        let piece = Self {
            position: pick(&self.position, nodes),
            orientation: pick(&self.orientation, nodes),
            velocity: pick(&self.velocity, nodes),
            angular_velocity: pick(&self.angular_velocity, nodes),
            load: pick(&self.load, nodes),
            torque: pick(&self.torque, nodes),
            drag: pick(&self.drag, nodes),
            dashpots: Vec::new(),
            added_mass: Vec::new(),
            gravity: self.gravity,
            planet: self.planet.clone(),
            rest: pick(&self.rest, nodes),
            mass: pick(&self.mass, nodes),
            inertia: pick(&self.inertia, nodes),
            free_translation: pick(&self.free_translation, nodes),
            free_rotation: pick(&self.free_rotation, nodes),
            elements,
            splits: self.splits,
        };
        (piece, kept)
    }
}
