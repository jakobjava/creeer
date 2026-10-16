//! Fill-reducing node ordering.
//!
//! Eliminating a node connects all its remaining neighbours to each other ("fill"). Nested
//! dissection keeps fill low: cut the structure in two with a small set of separator nodes,
//! order both halves first (recursively) and the separator last, so the halves never fill
//! into each other. Structures are physical, so the cut is made geometrically: split at the
//! median along the longest extent.
//!
//! The cuts also make parallelism possible: the two halves of a cut never touch each other
//! in the factor, so they can be factorised and solved at the same time ([`Dissection`]).

use creeer_math::glam::DVec3;

/// Pieces at most this big are not split further.
const LEAF: usize = 16;

/// The tree of cuts behind an ordering: which consecutive ranges of the order are
/// independent halves and which are separators.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dissection {
    /// `len` nodes, ordered without further cuts.
    Leaf(usize),
    /// Two independent halves (ordered first, left then right) and the separator after them.
    Cut(Box<Dissection>, Box<Dissection>, usize),
}

impl Dissection {
    /// Number of nodes covered.
    pub fn len(&self) -> usize {
        match self {
            Self::Leaf(len) => *len,
            Self::Cut(left, right, separator) => left.len() + right.len() + separator,
        }
    }

    /// Whether it covers no nodes.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The same tree with every node replaced by `factor` consecutive entries (e.g. one
    /// translation and one rotation block per node).
    pub fn scaled(&self, factor: usize) -> Self {
        match self {
            Self::Leaf(len) => Self::Leaf(len * factor),
            Self::Cut(left, right, separator) => Self::Cut(
                Box::new(left.scaled(factor)),
                Box::new(right.scaled(factor)),
                separator * factor,
            ),
        }
    }
}

/// Nested-dissection elimination order (`perm[new] = old`) for nodes at `positions`
/// connected by `edges`.
pub fn nested_dissection(positions: &[DVec3], edges: &[(usize, usize)]) -> Vec<usize> {
    nested_dissection_tree(positions, edges).0
}

/// [`nested_dissection`] plus its tree of cuts.
pub fn nested_dissection_tree(
    positions: &[DVec3],
    edges: &[(usize, usize)],
) -> (Vec<usize>, Dissection) {
    let n = positions.len();
    let mut adjacency = vec![Vec::new(); n];
    for &(a, b) in edges {
        adjacency[a].push(b);
        adjacency[b].push(a);
    }
    let mut state = State {
        positions,
        adjacency: &adjacency,
        mark: vec![0; n],
        next_mark: 0,
        order: Vec::with_capacity(n),
    };
    let tree = state.dissect((0..n).collect());
    (state.order, tree)
}

struct State<'a> {
    positions: &'a [DVec3],
    adjacency: &'a [Vec<usize>],
    /// `mark[node] == id` while `node` is on the left side of cut `id`.
    mark: Vec<u32>,
    next_mark: u32,
    order: Vec<usize>,
}

impl State<'_> {
    fn dissect(&mut self, mut nodes: Vec<usize>) -> Dissection {
        if nodes.len() <= LEAF {
            let len = nodes.len();
            self.order.extend(nodes);
            return Dissection::Leaf(len);
        }
        let (min, max) = nodes.iter().fold(
            (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
            |(lo, hi), &i| (lo.min(self.positions[i]), hi.max(self.positions[i])),
        );
        let extent = max - min;
        let axis = if extent.x >= extent.y && extent.x >= extent.z {
            0
        } else if extent.y >= extent.z {
            1
        } else {
            2
        };
        // Ties are broken by index, so the order is fully deterministic.
        nodes.sort_unstable_by(|&a, &b| {
            self.positions[a][axis]
                .total_cmp(&self.positions[b][axis])
                .then(a.cmp(&b))
        });
        let right = nodes.split_off(nodes.len() / 2);
        let left = nodes;

        self.next_mark += 1;
        let id = self.next_mark;
        for &i in &left {
            self.mark[i] = id;
        }
        // One-sided separator: right-side nodes touching the left side. Removing them leaves
        // no edge between the two halves.
        let (separator, rest): (Vec<usize>, Vec<usize>) = right
            .into_iter()
            .partition(|&i| self.adjacency[i].iter().any(|&j| self.mark[j] == id));
        let left = self.dissect(left);
        let right = self.dissect(rest);
        let separator_len = separator.len();
        self.order.extend(separator);
        Dissection::Cut(Box::new(left), Box::new(right), separator_len)
    }
}
