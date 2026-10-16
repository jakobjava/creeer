//! Numeric factorisation and solve, in parallel over the cuts of the nested dissection.
//!
//! The two halves of a cut never touch each other in L: a column in one half only reads
//! and writes columns of its own half (its descendants) and is read by the separator
//! (its ancestors). So both halves are processed at the same time, then the separator.
//!
//! - Factorisation, left-looking: column j reads its finished descendant columns and writes
//!   only itself.
//! - Forward substitution, "pull": y_j = L_jj⁻¹·(b_j − Σ_k L_jk·y_k) over descendants k.
//! - Backward substitution, "push": once x_j is known, x_k −= L_jkᵀ·x_j for descendants k.
//!   The separator goes first and pushes into both halves, then the halves run in parallel.
//!
//! Every value is computed by the same operations in the same order whether the halves run
//! in parallel or not, so the results are bit-identical on any number of threads.

use super::dense3;
use super::ordering::Dissection;
use super::{NotPositiveDefinite, SymbolicCholesky};
use creeer_math::glam::{DMat3, DVec3};

/// Cuts covering fewer blocks than this are processed on one thread (task overhead).
const GRAIN: usize = 192;

/// Inputs shared by all factorisation tasks.
pub(super) struct FactorInput<'a> {
    pub sym: &'a SymbolicCholesky,
    pub diag: &'a [DMat3],
}

/// Factorises the columns `lo..lo + tree.len()`. `l` holds their storage (starting at
/// storage index `col_ptr[lo]`), `diag_inv` their diagonal inverses.
pub(super) fn factor(
    input: &FactorInput<'_>,
    tree: &Dissection,
    lo: usize,
    l: &mut [DMat3],
    diag_inv: &mut [DMat3],
    slot: &mut Vec<usize>,
) -> Result<(), NotPositiveDefinite> {
    let sym = input.sym;
    let base = sym.col_ptr[lo];
    let hi = lo + tree.len();
    let separator = match tree {
        Dissection::Cut(left, right, separator) if tree.len() >= GRAIN => {
            let (m1, m2) = (lo + left.len(), lo + left.len() + right.len());
            let (l_left, l_rest) = l.split_at_mut(sym.col_ptr[m1] - base);
            let (l_right, _) = l_rest.split_at_mut(sym.col_ptr[m2] - sym.col_ptr[m1]);
            let (d_left, d_rest) = diag_inv.split_at_mut(m1 - lo);
            let (d_right, _) = d_rest.split_at_mut(m2 - m1);
            let blocks = slot.len();
            let (a, b) = rayon::join(
                || factor(input, left, lo, l_left, d_left, slot),
                || factor(input, right, m1, l_right, d_right, &mut vec![0; blocks]),
            );
            // The left half's failure is the one a sequential run would report first.
            a.and(b)?;
            hi - separator..hi
        }
        _ => lo..hi,
    };
    for j in separator {
        factor_column(input, j, base, l, &mut diag_inv[j - lo], slot)?;
    }
    Ok(())
}

/// Left-looking factorisation of column j (storage of `l` starts at index `base`).
fn factor_column(
    input: &FactorInput<'_>,
    j: usize,
    base: usize,
    l: &mut [DMat3],
    diag_inv: &mut DMat3,
    slot: &mut [usize],
) -> Result<(), NotPositiveDefinite> {
    let sym = input.sym;
    let column = sym.col_ptr[j]..sym.col_ptr[j + 1];
    for idx in column.clone() {
        slot[sym.row_idx[idx]] = idx;
    }
    let mut d = input.diag[sym.perm[j]];
    for &(k, entry) in &sym.row_entry[sym.row_ptr[j]..sym.row_ptr[j + 1]] {
        let ljk_t = l[entry - base].transpose();
        d -= l[entry - base] * ljk_t;
        // Rows below j in column k: all of them appear in column j as well (fill).
        for idx in entry + 1..sym.col_ptr[k + 1] {
            let target = slot[sym.row_idx[idx]];
            let update = l[idx - base] * ljk_t;
            l[target - base] -= update;
        }
    }
    let ljj = dense3::cholesky(d).ok_or(NotPositiveDefinite { node: sym.perm[j] })?;
    let inv = dense3::lower_inverse(ljj);
    *diag_inv = inv;
    let inv_t = inv.transpose();
    for idx in column {
        l[idx - base] *= inv_t;
    }
    Ok(())
}

/// The factor, for the solves.
pub(super) struct Factor<'a> {
    pub sym: &'a SymbolicCholesky,
    /// L's off-diagonal blocks in row order (row j: `row_entry[row_ptr[j]..row_ptr[j + 1]]`),
    /// so both substitutions read memory contiguously.
    pub rows: &'a [DMat3],
    pub diag_inv: &'a [DMat3],
}

/// Forward substitution on `x` = entries `lo..lo + tree.len()` of the permuted vector.
pub(super) fn forward(f: &Factor<'_>, tree: &Dissection, lo: usize, x: &mut [DVec3]) {
    let hi = lo + tree.len();
    let separator = match tree {
        Dissection::Cut(left, right, separator) if tree.len() >= GRAIN => {
            let m1 = lo + left.len();
            let (x_left, x_rest) = x.split_at_mut(left.len());
            let (x_right, _) = x_rest.split_at_mut(right.len());
            rayon::join(
                || forward(f, left, lo, x_left),
                || forward(f, right, m1, x_right),
            );
            hi - separator..hi
        }
        _ => lo..hi,
    };
    for j in separator {
        let mut b = x[j - lo];
        let range = f.sym.row_ptr[j]..f.sym.row_ptr[j + 1];
        for (&(k, _), l) in f.sym.row_entry[range.clone()].iter().zip(&f.rows[range]) {
            b -= *l * x[k - lo];
        }
        x[j - lo] = f.diag_inv[j] * b;
    }
}

/// Backward substitution on `x` = entries `lo..lo + tree.len()` of the permuted vector.
pub(super) fn backward(f: &Factor<'_>, tree: &Dissection, lo: usize, x: &mut [DVec3]) {
    let hi = lo + tree.len();
    let (separator, halves) = match tree {
        Dissection::Cut(left, right, separator) if tree.len() >= GRAIN => {
            (hi - separator..hi, Some((left, right)))
        }
        _ => (lo..hi, None),
    };
    for j in separator.rev() {
        let xj = f.diag_inv[j].transpose() * x[j - lo];
        x[j - lo] = xj;
        let range = f.sym.row_ptr[j]..f.sym.row_ptr[j + 1];
        for (&(k, _), l) in f.sym.row_entry[range.clone()].iter().zip(&f.rows[range]) {
            x[k - lo] -= l.transpose() * xj;
        }
    }
    if let Some((left, right)) = halves {
        let m1 = lo + left.len();
        let (x_left, x_rest) = x.split_at_mut(left.len());
        let (x_right, _) = x_rest.split_at_mut(right.len());
        rayon::join(
            || backward(f, left, lo, x_left),
            || backward(f, right, m1, x_right),
        );
    }
}
