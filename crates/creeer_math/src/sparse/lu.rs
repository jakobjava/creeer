//! Sparse LU factorisation with partial pivoting (Gilbert–Peierls, left-looking, as in
//! CSparse and KLU): P·A·Q = L·U, for the unsymmetric systems of circuits and pipe networks.
//!
//! - The columns are ordered by minimum degree on A + Aᵀ (`order.rs`) to keep the fill low.
//! - Each column's pivot is its diagonal if that is within [`PIVOT_TOLERANCE`] of the largest
//!   candidate (circuit matrices are mostly diagonally dominant: the ordering's fill estimate
//!   then holds), else the largest (lowest row on a tie).
//! - [`Lu::refactor`] reuses the pivots and patterns for new values (each step of a network
//!   whose topology did not change): no graph search, no pivot search, unless a pivot has
//!   become too small, when it factorises afresh.
//!
//! Both paths do the same arithmetic in the same order (a column's updates in ascending pivot
//! order), so they give the same bits; and neither depends on threads or the platform.

use super::order::minimum_degree;
use super::{Csc, Scalar};

/// A diagonal pivot is kept if it is at least this fraction of the column's largest candidate.
pub const PIVOT_TOLERANCE: f64 = 0.1;

const NONE: usize = usize::MAX;

/// The matrix is singular (to working precision) at a column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Singular {
    /// The column of A where no usable pivot was left.
    pub column: usize,
}

impl std::fmt::Display for Singular {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "singular matrix at column {}", self.column)
    }
}

impl std::error::Error for Singular {}

/// An LU factorisation.
#[derive(Clone, Debug)]
pub struct Lu<T> {
    n: usize,
    /// Step → column of A.
    q: Vec<usize>,
    /// Row of A → step, and step → row of A.
    pinv: Vec<usize>,
    p: Vec<usize>,
    /// L below its unit diagonal, by column (rows as steps).
    l_ptr: Vec<usize>,
    l_idx: Vec<usize>,
    l_val: Vec<T>,
    /// U above its diagonal, by column (rows as steps, ascending), and its diagonal.
    u_ptr: Vec<usize>,
    u_idx: Vec<usize>,
    u_val: Vec<T>,
    diag: Vec<T>,
    /// The pattern of A it was made for.
    a_ptr: Vec<usize>,
    a_idx: Vec<usize>,
    x: Vec<T>,
}

impl<T: Scalar> Lu<T> {
    /// Orders and factorises `a`.
    ///
    /// # Errors
    /// If `a` is singular.
    pub fn new(a: &Csc<T>) -> Result<Self, Singular> {
        let q = minimum_degree(a.n, &a.col_ptr, &a.row_idx);
        let mut lu = Self {
            n: a.n,
            q,
            pinv: Vec::new(),
            p: Vec::new(),
            l_ptr: Vec::new(),
            l_idx: Vec::new(),
            l_val: Vec::new(),
            u_ptr: Vec::new(),
            u_idx: Vec::new(),
            u_val: Vec::new(),
            diag: Vec::new(),
            a_ptr: a.col_ptr.clone(),
            a_idx: a.row_idx.clone(),
            x: vec![T::ZERO; a.n],
        };
        lu.factor(a)?;
        Ok(lu)
    }

    /// Rows and columns.
    pub fn n(&self) -> usize {
        self.n
    }

    /// Entries of L and U (a measure of the fill and of a solve's cost).
    pub fn nnz(&self) -> usize {
        self.l_idx.len() + self.u_idx.len() + self.n
    }

    /// Factorises `a` (same pattern as before, new values): with the same pivots if they
    /// still serve, else afresh. Returns whether it reused them.
    ///
    /// # Errors
    /// If `a` is singular.
    pub fn refactor(&mut self, a: &Csc<T>) -> Result<bool, Singular> {
        if a.col_ptr != self.a_ptr || a.row_idx != self.a_idx {
            *self = Self::new(a)?;
            return Ok(false);
        }
        if self.refactor_same(a) {
            Ok(true)
        } else {
            self.factor(a).map(|()| false)
        }
    }

    /// The full factorisation, with pivot search.
    fn factor(&mut self, a: &Csc<T>) -> Result<(), Singular> {
        let n = self.n;
        self.pinv = vec![NONE; n];
        self.p = vec![NONE; n];
        let (mut l_ptr, mut l_idx, mut l_val): (Vec<usize>, Vec<usize>, Vec<T>) =
            (vec![0], Vec::new(), Vec::new());
        let (mut u_ptr, mut u_idx, mut u_val): (Vec<usize>, Vec<usize>, Vec<T>) =
            (vec![0], Vec::new(), Vec::new());
        let mut diag = Vec::with_capacity(n);
        let x = &mut self.x;
        let mut mark = vec![false; n];
        let mut touched: Vec<usize> = Vec::new();
        let mut reached: Vec<usize> = Vec::new();
        let mut stack: Vec<usize> = Vec::new();
        let mut seen = vec![false; n];
        for k in 0..n {
            let col = self.q[k];
            // The steps whose L columns this column's solve goes through: from its rows,
            // along L's columns (depth first; then in ascending order, a valid one).
            reached.clear();
            for &r in &a.row_idx[a.col_ptr[col]..a.col_ptr[col + 1]] {
                let s = self.pinv[r];
                if s != NONE && !seen[s] {
                    seen[s] = true;
                    stack.push(s);
                    while let Some(j) = stack.pop() {
                        reached.push(j);
                        for &i in &l_idx[l_ptr[j]..l_ptr[j + 1]] {
                            let si = self.pinv[i];
                            if si != NONE && !seen[si] {
                                seen[si] = true;
                                stack.push(si);
                            }
                        }
                    }
                }
            }
            reached.sort_unstable();
            for &s in &reached {
                seen[s] = false;
            }
            // x = L⁻¹·A(:, col), by rows of A.
            touched.clear();
            for p in a.col_ptr[col]..a.col_ptr[col + 1] {
                let r = a.row_idx[p];
                x[r] = a.values[p];
                mark[r] = true;
                touched.push(r);
            }
            for &j in &reached {
                let xj = x[self.p[j]];
                u_idx.push(j);
                u_val.push(xj);
                for t in l_ptr[j]..l_ptr[j + 1] {
                    let i = l_idx[t];
                    if !mark[i] {
                        mark[i] = true;
                        touched.push(i);
                    }
                    x[i] -= l_val[t] * xj;
                }
            }
            u_ptr.push(u_idx.len());
            // The pivot among the rows not yet pivoted.
            touched.sort_unstable();
            let mut best: Option<(f64, usize)> = None;
            for &r in &touched {
                if self.pinv[r] == NONE {
                    let m = x[r].magnitude();
                    if best.is_none_or(|(b, _)| m > b) {
                        best = Some((m, r));
                    }
                }
            }
            let Some((largest, mut row)) = best.filter(|(m, _)| *m > 0.0 && m.is_finite()) else {
                for &r in &touched {
                    x[r] = T::ZERO;
                    mark[r] = false;
                }
                return Err(Singular { column: col });
            };
            if self.pinv[col] == NONE && x[col].magnitude() >= PIVOT_TOLERANCE * largest {
                row = col;
            }
            let pivot = x[row];
            self.pinv[row] = k;
            self.p[k] = row;
            diag.push(pivot);
            for &r in &touched {
                if self.pinv[r] == NONE {
                    l_idx.push(r);
                    l_val.push(x[r] / pivot);
                }
                x[r] = T::ZERO;
                mark[r] = false;
            }
            l_ptr.push(l_idx.len());
        }
        for i in &mut l_idx {
            *i = self.pinv[*i];
        }
        (self.l_ptr, self.l_idx, self.l_val) = (l_ptr, l_idx, l_val);
        (self.u_ptr, self.u_idx, self.u_val) = (u_ptr, u_idx, u_val);
        self.diag = diag;
        Ok(())
    }

    /// New values, the same pivots: false if a pivot is no longer good enough.
    fn refactor_same(&mut self, a: &Csc<T>) -> bool {
        let x = &mut self.x;
        for k in 0..self.n {
            let col = self.q[k];
            for p in a.col_ptr[col]..a.col_ptr[col + 1] {
                x[self.pinv[a.row_idx[p]]] = a.values[p];
            }
            for t in self.u_ptr[k]..self.u_ptr[k + 1] {
                let j = self.u_idx[t];
                let xj = x[j];
                self.u_val[t] = xj;
                for s in self.l_ptr[j]..self.l_ptr[j + 1] {
                    x[self.l_idx[s]] -= self.l_val[s] * xj;
                }
            }
            let pivot = x[k];
            let below = self.l_ptr[k]..self.l_ptr[k + 1];
            let largest = self.l_idx[below.clone()]
                .iter()
                .map(|&i| x[i].magnitude())
                .fold(pivot.magnitude(), f64::max);
            let ok = pivot.magnitude() > 0.0
                && pivot.magnitude() >= PIVOT_TOLERANCE * largest
                && largest.is_finite();
            if ok {
                self.diag[k] = pivot;
                for s in below.clone() {
                    self.l_val[s] = x[self.l_idx[s]] / pivot;
                }
            }
            for t in self.u_ptr[k]..self.u_ptr[k + 1] {
                x[self.u_idx[t]] = T::ZERO;
            }
            for s in below {
                x[self.l_idx[s]] = T::ZERO;
            }
            x[k] = T::ZERO;
            if !ok {
                return false;
            }
        }
        true
    }

    /// Solves A·x = b in place (`b` holds x on return).
    ///
    /// # Panics
    /// If `b` does not have n entries.
    pub fn solve(&mut self, b: &mut [T]) {
        assert_eq!(b.len(), self.n, "one entry per row");
        let y = &mut self.x;
        for k in 0..self.n {
            y[k] = b[self.p[k]];
        }
        for j in 0..self.n {
            let yj = y[j];
            for s in self.l_ptr[j]..self.l_ptr[j + 1] {
                y[self.l_idx[s]] -= self.l_val[s] * yj;
            }
        }
        for k in (0..self.n).rev() {
            let yk = y[k] / self.diag[k];
            y[k] = yk;
            for t in self.u_ptr[k]..self.u_ptr[k + 1] {
                y[self.u_idx[t]] -= self.u_val[t] * yk;
            }
        }
        for k in 0..self.n {
            b[self.q[k]] = y[k];
            y[k] = T::ZERO;
        }
    }
}
