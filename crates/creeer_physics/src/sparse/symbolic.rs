//! Symbolic analysis: elimination tree and the non-zero pattern of the factor.

use super::ordering::{Dissection, nested_dissection_tree};
use creeer_math::glam::DVec3;

const NONE: usize = usize::MAX;

/// The pattern of `L` for a given connectivity and node ordering. Computed once per structure
/// and reused for every numeric factorisation.
#[derive(Clone, Debug)]
pub struct SymbolicCholesky {
    /// `perm[new] = old` node index.
    pub(super) perm: Vec<usize>,
    /// The cuts behind the ordering (for parallel numeric work).
    pub(super) tree: Dissection,
    /// Column j of L below the diagonal: rows `row_idx[col_ptr[j]..col_ptr[j + 1]]`, ascending.
    pub(super) col_ptr: Vec<usize>,
    pub(super) row_idx: Vec<usize>,
    /// Row j of L left of the diagonal: `(k, storage index of L(j, k))`, k ascending.
    pub(super) row_ptr: Vec<usize>,
    pub(super) row_entry: Vec<(usize, usize)>,
    /// Per input edge: storage index of its block and whether it lands transposed.
    pub(super) edge_slot: Vec<(usize, bool)>,
}

impl SymbolicCholesky {
    /// Analyses a structure with nodes at `positions` connected by `edges`, using a
    /// geometric nested-dissection ordering to keep the factor sparse (and to let the
    /// numeric work run in parallel over its cuts).
    pub fn new(positions: &[DVec3], edges: &[(usize, usize)]) -> Self {
        let (perm, tree) = nested_dissection_tree(positions, edges);
        Self::with_dissection(edges, perm, tree)
    }

    /// Analyses with a given elimination order (`perm[new] = old`), without parallelism.
    ///
    /// # Panics
    /// If `perm` is not a permutation or an edge connects a node to itself.
    pub fn with_ordering(edges: &[(usize, usize)], perm: Vec<usize>) -> Self {
        let tree = Dissection::Leaf(perm.len());
        Self::with_dissection(edges, perm, tree)
    }

    /// Analyses with a given order and the nested-dissection tree it came from.
    ///
    /// # Panics
    /// If `perm` is not a permutation, the tree does not cover it, or an edge connects a
    /// node to itself.
    pub fn with_dissection(edges: &[(usize, usize)], perm: Vec<usize>, tree: Dissection) -> Self {
        assert_eq!(
            tree.len(),
            perm.len(),
            "the dissection tree must cover the order"
        );
        let n = perm.len();
        let mut inv = vec![NONE; n];
        for (new, &old) in perm.iter().enumerate() {
            assert!(inv[old] == NONE, "node {old} appears twice in the ordering");
            inv[old] = new;
        }
        // Lower-triangular adjacency in the new numbering: lower[i] holds k < i.
        let mut lower = vec![Vec::new(); n];
        for &(a, b) in edges {
            assert_ne!(a, b, "edge connects node {a} to itself");
            let (pa, pb) = (inv[a], inv[b]);
            lower[pa.max(pb)].push(pa.min(pb));
        }
        let parent = elimination_tree(&lower);

        // Row i of L contains every node on the tree paths from its neighbours k < i up to i.
        let mut mark = vec![NONE; n];
        let mut rows: Vec<Vec<usize>> = Vec::with_capacity(n);
        for (i, neighbours) in lower.iter().enumerate() {
            mark[i] = i;
            let mut row = Vec::new();
            for &k in neighbours {
                let mut r = k;
                while mark[r] != i {
                    row.push(r);
                    mark[r] = i;
                    r = parent[r];
                }
            }
            row.sort_unstable();
            rows.push(row);
        }

        let mut col_ptr = vec![0; n + 1];
        for row in &rows {
            for &k in row {
                col_ptr[k + 1] += 1;
            }
        }
        for j in 0..n {
            col_ptr[j + 1] += col_ptr[j];
        }
        let mut cursor = col_ptr.clone();
        let mut row_idx = vec![0; col_ptr[n]];
        let mut row_ptr = Vec::with_capacity(n + 1);
        let mut row_entry = Vec::with_capacity(col_ptr[n]);
        row_ptr.push(0);
        // Rows are visited in ascending order, so every column's row list comes out sorted.
        for (i, row) in rows.iter().enumerate() {
            for &k in row {
                row_idx[cursor[k]] = i;
                row_entry.push((k, cursor[k]));
                cursor[k] += 1;
            }
            row_ptr.push(row_entry.len());
        }

        let find = |col: usize, row: usize| {
            let range = col_ptr[col]..col_ptr[col + 1];
            let offset = row_idx[range.clone()]
                .binary_search(&row)
                .expect("edge missing from the factor pattern");
            range.start + offset
        };
        let edge_slot = edges
            .iter()
            .map(|&(a, b)| {
                let (pa, pb) = (inv[a], inv[b]);
                // Storage holds L(i, j) for i > j; block A(a, b) belongs at (pa, pb).
                if pa > pb {
                    (find(pb, pa), false)
                } else {
                    (find(pa, pb), true)
                }
            })
            .collect();

        Self {
            perm,
            col_ptr,
            row_idx,
            row_ptr,
            row_entry,
            edge_slot,
            tree,
        }
    }

    /// Number of nodes (block rows).
    pub fn nodes(&self) -> usize {
        self.perm.len()
    }

    /// Off-diagonal 3×3 blocks stored in `L` (a measure of fill and of solve cost).
    pub fn factor_blocks(&self) -> usize {
        self.row_idx.len()
    }

    /// Block multiply-adds one numeric factorisation performs (a measure of its cost).
    pub fn factor_work(&self) -> usize {
        self.row_entry
            .iter()
            .map(|&(k, entry)| self.col_ptr[k + 1] - entry)
            .sum()
    }
}

/// Elimination tree (Liu's algorithm with path compression): `parent[j]` is the first row
/// below j that is non-zero in column j of L, or `NONE` for roots.
fn elimination_tree(lower: &[Vec<usize>]) -> Vec<usize> {
    let n = lower.len();
    let mut parent = vec![NONE; n];
    let mut ancestor = vec![NONE; n];
    for (i, neighbours) in lower.iter().enumerate() {
        for &k in neighbours {
            let mut r = k;
            while ancestor[r] != NONE && ancestor[r] != i {
                let next = ancestor[r];
                ancestor[r] = i;
                r = next;
            }
            if ancestor[r] == NONE {
                ancestor[r] = i;
                parent[r] = i;
            }
        }
    }
    parent
}
