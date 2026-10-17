//! The numbers of a Cholesky factorisation, and solves with it, in parallel over independent
//! subtrees of the elimination tree.
//!
//! In postorder every subtree is a contiguous range of rows, and a row only reads rows of its
//! own subtree. So the subtrees small enough to be "tasks" (`Cholesky::in_task`) are done all
//! at once, grouped into contiguous jobs, and then the rows above them, in order. Each row is
//! computed by the same operations in the same order whichever thread does it: the results
//! have the same bits on any number of threads.
//!
//! Forward substitution "pulls" (row k sums its own entries: it reads only its subtree);
//! backward substitution goes down from the top rows first, then into the subtrees in
//! parallel, each reading the top rows' final values.

use super::{Cholesky, NotPositiveDefinite};
use crate::sparse::Csc;
use rayon::prelude::*;

impl Cholesky {
    /// Splits a row-indexed array into the jobs' parts.
    fn split_rows<'a, T>(&self, all: &'a mut [T]) -> Vec<&'a mut [T]> {
        let mut parts = Vec::with_capacity(self.jobs.len());
        let mut rest = all;
        for &(lo, hi) in &self.jobs {
            let (part, after) = rest.split_at_mut(hi - lo);
            parts.push(part);
            rest = after;
        }
        parts
    }

    /// Splits a column-indexed array (`ptr` its starts) into the jobs' parts.
    fn split<'a, T>(&self, all: &'a mut [T], ptr: &[usize]) -> Vec<&'a mut [T]> {
        let mut parts = Vec::with_capacity(self.jobs.len());
        let (mut rest, mut start) = (all, 0);
        for &(_, hi) in &self.jobs {
            let (part, after) = rest.split_at_mut(ptr[hi] - start);
            parts.push(part);
            rest = after;
            start = ptr[hi];
        }
        parts
    }

    /// Factorises `a` (the pattern the factorisation was made for).
    pub(super) fn factor(&mut self, a: &Csc<f64>) -> Result<(), NotPositiveDefinite> {
        let mut l_val = std::mem::take(&mut self.l_val);
        let mut scratch = std::mem::take(&mut self.scratch);
        let mut x = std::mem::take(&mut self.x);
        let result = self.factor_with(a, &mut l_val, &mut scratch, &mut x);
        (self.l_val, self.scratch, self.x) = (l_val, scratch, x);
        result
    }

    fn factor_with(
        &self,
        a: &Csc<f64>,
        l_val: &mut [f64],
        scratch: &mut [Vec<f64>],
        x: &mut [f64],
    ) -> Result<(), NotPositiveDefinite> {
        if !self.jobs.is_empty() {
            self.factor_tasks(a, l_val, scratch)?;
        }
        for k in (0..self.n).filter(|&k| !self.in_task[k]) {
            self.row(k, a, (l_val, 0), (x, 0))?;
        }
        Ok(())
    }

    /// The subtrees' rows, in parallel (a small factorisation has none: no tasks started).
    fn factor_tasks(
        &self,
        a: &Csc<f64>,
        l_val: &mut [f64],
        scratch: &mut [Vec<f64>],
    ) -> Result<(), NotPositiveDefinite> {
        let parts = self.split(l_val, &self.l_ptr);
        let failed: Vec<NotPositiveDefinite> = self
            .jobs
            .par_iter()
            .zip(parts)
            .zip(scratch)
            .filter_map(|((&(lo, hi), part), xs)| {
                let base = self.l_ptr[lo];
                (lo..hi)
                    .filter(|&k| self.in_task[k])
                    .find_map(|k| self.row(k, a, (part, base), (xs, lo)).err())
            })
            .collect();
        failed.first().map_or(Ok(()), |&e| Err(e))
    }

    /// Row k of L (up-looking): `l_val` holds L's values from storage index `l_base`, `x` is
    /// scratch for rows from `x_base` (all zero on entry and on return).
    fn row(
        &self,
        k: usize,
        a: &Csc<f64>,
        (l_val, l_base): (&mut [f64], usize),
        (x, x_base): (&mut [f64], usize),
    ) -> Result<(), NotPositiveDefinite> {
        for p in self.c_ptr[k]..self.c_ptr[k + 1] {
            x[self.c_idx[p] - x_base] = a.values[self.c_from[p]];
        }
        let mut d = x[k - x_base];
        x[k - x_base] = 0.0;
        for t in self.r_ptr[k]..self.r_ptr[k + 1] {
            let (i, slot) = (self.r_idx[t], self.r_slot[t]);
            let lki = x[i - x_base] / l_val[self.l_ptr[i] - l_base];
            x[i - x_base] = 0.0;
            // Column i's entries above row k (rows between: computed already).
            for p in self.l_ptr[i] + 1..slot {
                x[self.l_idx[p] - x_base] -= l_val[p - l_base] * lki;
            }
            d -= lki * lki;
            l_val[slot - l_base] = lki;
        }
        if d <= 0.0 || !d.is_finite() {
            return Err(NotPositiveDefinite { row: self.perm[k] });
        }
        l_val[self.l_ptr[k] - l_base] = d.sqrt();
        Ok(())
    }

    /// y_k = (y_k − Σ L(k, i)·y_i) / L(k, k): `y` holds rows from `base`.
    fn forward_row(&self, k: usize, y: &mut [f64], base: usize) {
        let mut s = y[k - base];
        for t in self.r_ptr[k]..self.r_ptr[k + 1] {
            s -= self.l_val[self.r_slot[t]] * y[self.r_idx[t] - base];
        }
        y[k - base] = s / self.l_val[self.l_ptr[k]];
    }

    /// x_j = (y_j − Σ L(p, j)·x_p) / L(j, j) over the rows p below j: `own` holds rows from
    /// `base` to `base + own.len()`, `rest` the rows beyond.
    fn backward_row(&self, j: usize, own: &mut [f64], base: usize, rest: &[f64]) {
        let mut s = own[j - base];
        for p in self.l_ptr[j] + 1..self.l_ptr[j + 1] {
            let r = self.l_idx[p];
            let xr = if r < base + own.len() {
                own[r - base]
            } else {
                rest[r]
            };
            s -= self.l_val[p] * xr;
        }
        own[j - base] = s / self.l_val[self.l_ptr[j]];
    }

    /// Solves A·x = b in place.
    ///
    /// # Panics
    /// If `b` does not have n entries.
    pub fn solve(&mut self, b: &mut [f64]) {
        assert_eq!(b.len(), self.n, "one entry per row");
        let mut y = std::mem::take(&mut self.x);
        for k in 0..self.n {
            y[k] = b[self.perm[k]];
        }

        // Forward: the subtrees, then the rows above them. (A small factorisation has no
        // subtree tasks: no tasks started, they cost more than its rows.)
        if !self.jobs.is_empty() {
            let parts = self.split_rows(&mut y);
            self.jobs
                .par_iter()
                .zip(parts)
                .for_each(|(&(lo, hi), part)| {
                    for k in (lo..hi).filter(|&k| self.in_task[k]) {
                        self.forward_row(k, part, lo);
                    }
                });
        }
        for k in (0..self.n).filter(|&k| !self.in_task[k]) {
            self.forward_row(k, &mut y, 0);
        }
        // Backward: the rows above, then the subtrees (reading the rows above).
        for j in (0..self.n).rev().filter(|&j| !self.in_task[j]) {
            self.backward_row(j, &mut y, 0, &[]);
        }
        if self.in_task.iter().any(|&t| t) {
            let mut above = std::mem::take(&mut self.above);
            above.clone_from(&y);
            let parts = self.split_rows(&mut y);
            self.jobs
                .par_iter()
                .zip(parts)
                .for_each(|(&(lo, hi), part)| {
                    for j in (lo..hi).rev().filter(|&j| self.in_task[j]) {
                        self.backward_row(j, part, lo, &above);
                    }
                });
            self.above = above;
        }
        for k in 0..self.n {
            b[self.perm[k]] = y[k];
            y[k] = 0.0;
        }
        self.x = y;
    }
}
