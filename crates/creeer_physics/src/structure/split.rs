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

impl Structure {
    /// The least elastic energy that yields a member or joint at node `node` (J): over its
    /// members there, each measure's capacity c and stiffness k at c²/(2k) (none: infinite).
    /// An impact bringing far less cannot make anything there flow (`stepping.rs`).
    pub fn yield_energy(&self, node: NodeId) -> f64 {
        (self.elements.iter())
            .filter(|e| !e.broken && (e.a == node || e.b == node))
            .flat_map(|e| {
                let caps = e.capacities(&[0.0; super::element::MEASURES], 1.0);
                (0..super::element::MEASURES).map(move |m| {
                    let k = e.stiffness.entry(m, m);
                    let c = caps[m].capacity;
                    if k > 0.0 {
                        c * c / (2.0 * k)
                    } else {
                        f64::INFINITY
                    }
                })
            })
            .fold(f64::INFINITY, f64::min)
    }
}

impl Structure {
    /// Whether node `node` is held fast: neither moving nor turning (a post's ground).
    pub fn held_fast(&self, node: NodeId) -> bool {
        use creeer_math::glam::DVec3;
        self.free_translation[node] == DVec3::ZERO && self.free_rotation[node] == DVec3::ZERO
    }
}

impl Structure {
    /// Severs element `e` (a member sawn through, a joint's fasteners pulled, G2.6): it carries
    /// nothing from now on, and the structure may fall apart (`World::fall_apart`). Its solver
    /// must be refactored (`ImplicitSolver::refactor`).
    pub fn sever(&mut self, e: usize) {
        if let Some(el) = self.elements.get_mut(e) {
            el.broken = true;
            el.stiffness = super::element::Stiffness::zeroed();
        }
    }
}

impl Structure {
    /// Element `e`'s two nodes if it is a joint and holds (not broken).
    pub fn joint_ends(&self, e: usize) -> Option<(NodeId, NodeId)> {
        let el = self.elements.get(e)?;
        (el.is_joint() && !el.broken).then_some((el.a, el.b))
    }
}

impl Structure {
    /// Cuts member `e` through at `t` of its length (0 < t < 1; a saw's or an axe's cut, G2.6):
    /// a node there on each side of the cut, each piece of the member whole on its own side,
    /// each node with its own piece's mass and turning inertia. Returns the two nodes.
    ///
    /// # Panics
    /// If `t` is not within the member, or the member is a joint.
    pub fn cut_at(&mut self, e: usize, t: f64) -> (NodeId, NodeId) {
        let m = self.insert_node(e, t);
        let second = self.element_count() - 1;
        let (first, rest) = (self.elements[e], self.elements[second]);
        let n = self.node_count();
        self.position.push(self.position[m]);
        self.orientation.push(self.orientation[m]);
        self.velocity.push(self.velocity[m]);
        self.angular_velocity.push(self.angular_velocity[m]);
        self.load.push(creeer_math::glam::DVec3::ZERO);
        self.torque.push(creeer_math::glam::DVec3::ZERO);
        self.drag.push(0.0);
        self.rest.push(self.rest[m]);
        self.free_translation.push(creeer_math::glam::DVec3::ONE);
        self.free_rotation.push(creeer_math::glam::DVec3::ONE);
        // Each side's node lumps its own piece's half.
        self.mass[m] = 0.5 * first.mass();
        self.mass.push(0.5 * rest.mass());
        self.inertia[m] = first.node_inertia();
        self.inertia.push(rest.node_inertia());
        self.elements[second].a = n;
        (m, n)
    }
}
