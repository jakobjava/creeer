//! The connected parts of a network.
//!
//! Edges to a fixed potential (the air around, electrical ground, a pressure reservoir) do not
//! join islands: they are boundary conditions, and callers leave them out.

/// The islands of a network: every node's island, and each island's nodes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Islands {
    of: Vec<usize>,
    local: Vec<usize>,
    members: Vec<Vec<usize>>,
}

fn root(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

impl Islands {
    /// The islands of `nodes` nodes joined by `edges`. Islands are numbered by their lowest
    /// node, nodes within each in ascending order: the same however the edges are listed.
    ///
    /// # Panics
    /// If an edge names a node that does not exist.
    pub fn new(nodes: usize, edges: impl IntoIterator<Item = (usize, usize)>) -> Self {
        let mut parent: Vec<usize> = (0..nodes).collect();
        for (a, b) in edges {
            assert!(
                a < nodes && b < nodes,
                "an edge to a node that does not exist"
            );
            let (ra, rb) = (root(&mut parent, a), root(&mut parent, b));
            // The lower root wins: the result does not depend on the order of the edges.
            if ra != rb {
                let (lo, hi) = (ra.min(rb), ra.max(rb));
                parent[hi] = lo;
            }
        }
        let mut of = vec![usize::MAX; nodes];
        let mut local = vec![0; nodes];
        let mut members: Vec<Vec<usize>> = Vec::new();
        let mut island_of_root = vec![usize::MAX; nodes];
        for i in 0..nodes {
            let r = root(&mut parent, i);
            if island_of_root[r] == usize::MAX {
                island_of_root[r] = members.len();
                members.push(Vec::new());
            }
            let island = island_of_root[r];
            of[i] = island;
            local[i] = members[island].len();
            members[island].push(i);
        }
        Self { of, local, members }
    }

    /// How many islands.
    pub fn count(&self) -> usize {
        self.members.len()
    }

    /// Node `node`'s island.
    pub fn island_of(&self, node: usize) -> usize {
        self.of[node]
    }

    /// Node `node`'s index within its island.
    pub fn local(&self, node: usize) -> usize {
        self.local[node]
    }

    /// Island `island`'s nodes, ascending.
    pub fn members(&self, island: usize) -> &[usize] {
        &self.members[island]
    }
}
