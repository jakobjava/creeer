//! Sparse Cholesky factorisation (A = L·Lᵀ) of symmetric positive-definite matrices made of
//! 3×3 blocks, one block row and column per node (its x, y, z), as produced by structures.
//!
//! Used for exact static solves (ADR-0002) and implicit time stepping (ADR-0004).
//!
//! The work is split in two, so the expensive part runs only when the structure changes:
//! - [`SymbolicCholesky`]: fill-reducing ordering, elimination tree and the pattern of `L`
//!   (depends only on which nodes are connected);
//! - [`Cholesky::factor`]: the numbers (depends on stiffness, masses and the time step).
//!
//! **Deterministic by construction:** every value is computed by the same operations in a
//! fixed order, so the result has the same bits on every platform and thread count. The
//! block kernels (`kernels.rs`) put a block's three rows in SIMD lanes (AVX2 chosen at run
//! time, NEON on aarch64), each lane doing the scalar expression's operations; nothing is
//! summed across lanes and nothing is fused. General-purpose libraries pick their summation
//! order from the CPU's vector width, which would break the cross-platform golden hashes.

use creeer_math::glam::{DMat3, DVec3};
use std::fmt;

mod dense3;
#[cfg(test)]
mod kernel_tests;
mod kernels;
mod numeric;
mod ordering;
mod symbolic;
#[cfg(test)]
mod tests;

pub use ordering::{Dissection, nested_dissection, nested_dissection_tree};
pub use symbolic::SymbolicCholesky;

/// The matrix is not (numerically) positive definite: the structure is a mechanism (it can
/// move without deforming anything) or a block was assembled wrongly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotPositiveDefinite {
    /// Node (original numbering) where the factorisation broke down.
    pub node: usize,
}

impl fmt::Display for NotPositiveDefinite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "matrix is not positive definite at node {}", self.node)
    }
}

impl std::error::Error for NotPositiveDefinite {}

/// A numeric factorisation `A = L·Lᵀ` that can be refactored with new values and reused for
/// many solves. Runs in parallel over the cuts of the nested dissection (`numeric.rs`).
///
/// A supernodal variant (dense column panels, as in CHOLMOD) was measured slower than these
/// 3×3 blocks without SIMD kernels: solve 1.9 ms vs 1.3 ms, factorisation 174 ms vs 106 ms
/// for a 4,864-beam welded frame (ADR-0004).
#[derive(Clone, Debug)]
pub struct Cholesky {
    sym: SymbolicCholesky,
    /// Off-diagonal blocks of L, in the symbolic storage order.
    l: Vec<DMat3>,
    /// The same blocks in row order, for both substitutions. One copy for both (not a
    /// transposed one for the backward pass): the backward pass then starts on the blocks
    /// the forward one read last, still in the cache, and eight bridges solving at once
    /// spent 5.4 ms a step in their solves against 7.0 with two copies (ADR-0035).
    rows: Vec<DMat3>,
    /// Inverses of the (lower-triangular) diagonal blocks of L.
    diag_inv: Vec<DMat3>,
    /// Scratch: storage index of row i in the current column.
    slot: Vec<usize>,
    /// Scratch: permuted right-hand side.
    work: Vec<DVec3>,
}

impl Cholesky {
    /// Allocates storage for matrices with the pattern of `sym`. Call [`Self::factor`] next.
    pub fn new(sym: SymbolicCholesky) -> Self {
        let n = sym.nodes();
        Self {
            l: vec![DMat3::ZERO; sym.row_idx.len()],
            rows: vec![DMat3::ZERO; sym.row_idx.len()],
            diag_inv: vec![DMat3::IDENTITY; n],
            slot: vec![usize::MAX; n],
            work: vec![DVec3::ZERO; n],
            sym,
        }
    }

    /// The pattern this factorisation uses.
    pub fn symbolic(&self) -> &SymbolicCholesky {
        &self.sym
    }

    /// Factors the matrix with diagonal blocks `diag[node]` and off-diagonal blocks `off[e]`.
    /// `off[e]` is the block A(a, b) of edge `e = (a, b)` given to the symbolic analysis
    /// (A(b, a) is its transpose). Edges listed more than once are summed.
    ///
    /// # Errors
    /// If the matrix is not positive definite.
    ///
    /// # Panics
    /// If the slices do not match the symbolic pattern.
    pub fn factor(&mut self, diag: &[DMat3], off: &[DMat3]) -> Result<(), NotPositiveDefinite> {
        let sym = &self.sym;
        assert_eq!(diag.len(), sym.nodes(), "one diagonal block per node");
        assert_eq!(off.len(), sym.edge_slot.len(), "one block per edge");
        self.l.fill(DMat3::ZERO);
        for (block, &(slot, transposed)) in off.iter().zip(&sym.edge_slot) {
            self.l[slot] += if transposed {
                block.transpose()
            } else {
                *block
            };
        }
        let input = numeric::FactorInput { sym, diag };
        numeric::factor(
            &input,
            &sym.tree,
            0,
            &mut self.l,
            &mut self.diag_inv,
            &mut self.slot,
        )?;
        for (row, &(_, entry)) in self.rows.iter_mut().zip(&sym.row_entry) {
            *row = self.l[entry];
        }
        Ok(())
    }

    /// Solves `A·x = b` in place (`rhs` holds b on entry and x on return), using the last
    /// successful [`Self::factor`].
    ///
    /// # Panics
    /// If `rhs` does not have one entry per node.
    pub fn solve(&mut self, rhs: &mut [DVec3]) {
        let sym = &self.sym;
        assert_eq!(rhs.len(), sym.nodes(), "one right-hand side per node");
        for (j, w) in self.work.iter_mut().enumerate() {
            *w = rhs[sym.perm[j]];
        }
        let factor = numeric::Factor {
            sym,
            rows: &self.rows,
            diag_inv: &self.diag_inv,
        };
        numeric::forward(&factor, &sym.tree, 0, &mut self.work);
        numeric::backward(&factor, &sym.tree, 0, &mut self.work);
        for (j, w) in self.work.iter().enumerate() {
            rhs[sym.perm[j]] = *w;
        }
    }
}
