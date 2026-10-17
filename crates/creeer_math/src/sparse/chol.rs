//! Sparse Cholesky factorisation P·A·Pᵀ = L·Lᵀ of symmetric positive-definite matrices (heat
//! conduction, resistor networks): half the work and memory of LU, no pivoting needed.
//!
//! Up-looking, as in CSparse: the symbolic part (minimum-degree order, elimination tree, the
//! pattern of L) depends only on the matrix's pattern and is kept; [`Cholesky::refactor`]
//! computes the numbers for new values. Plain arithmetic in a fixed order: the same bits on
//! every platform.

mod numeric;

use super::Csc;
use super::order::minimum_degree;

const NONE: usize = usize::MAX;

/// The matrix is not (numerically) positive definite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotPositiveDefinite {
    /// The row of A where the factorisation broke down.
    pub row: usize,
}

impl std::fmt::Display for NotPositiveDefinite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "matrix not positive definite at row {}", self.row)
    }
}

impl std::error::Error for NotPositiveDefinite {}

/// A Cholesky factorisation.
#[derive(Clone, Debug)]
pub struct Cholesky {
    n: usize,
    /// Step → row of A.
    perm: Vec<usize>,
    /// The upper triangle of P·A·Pᵀ by column: its entries come from A's entries `c_from`.
    c_ptr: Vec<usize>,
    c_idx: Vec<usize>,
    c_from: Vec<usize>,
    parent: Vec<usize>,
    /// L by column, its diagonal first.
    l_ptr: Vec<usize>,
    l_idx: Vec<usize>,
    l_val: Vec<f64>,
    a_ptr: Vec<usize>,
    a_idx: Vec<usize>,
    x: Vec<f64>,
    stack: Vec<usize>,
    path: Vec<usize>,
    mark: Vec<usize>,
    /// Each row k's reach (the columns i < k where L(k, i) is non-zero, each after the ones
    /// it depends on), and where in L each of those entries goes: the pattern only, so kept.
    r_ptr: Vec<usize>,
    r_idx: Vec<usize>,
    r_slot: Vec<usize>,
    /// Whether a row is in a subtree done in parallel (`numeric.rs`), the jobs (contiguous
    /// ranges of rows covering all, each holding whole subtrees) and their scratch.
    in_task: Vec<bool>,
    jobs: Vec<(usize, usize)>,
    scratch: Vec<Vec<f64>>,
    /// Multiply-adds of a factorisation: column j's count c gives (c² − c)/2.
    flops: usize,
    /// The solve's copy of the rows above the subtrees.
    above: Vec<f64>,
}

impl Cholesky {
    /// Orders and factorises `a` (symmetric; both triangles stored, as [`super::Assembly`]
    /// makes them).
    ///
    /// # Errors
    /// If `a` is not positive definite.
    pub fn new(a: &Csc<f64>) -> Result<Self, NotPositiveDefinite> {
        let n = a.n;
        // Minimum degree, then its elimination tree in postorder: the same fill, and every
        // subtree a contiguous range of rows, so independent subtrees can be done in parallel.
        let order = minimum_degree(n, &a.col_ptr, &a.row_idx);
        let (_, _, _, tree) = permuted(a, &order);
        let perm: Vec<usize> = postorder(&tree).iter().map(|&k| order[k]).collect();
        let (c_ptr, c_idx, c_from, parent) = permuted(a, &perm);
        let mut chol = Self {
            n,
            perm,
            c_ptr,
            c_idx,
            c_from,
            parent,
            l_ptr: Vec::new(),
            l_idx: Vec::new(),
            l_val: Vec::new(),
            a_ptr: a.col_ptr.clone(),
            a_idx: a.row_idx.clone(),
            x: vec![0.0; n],
            stack: vec![0; n],
            path: vec![0; n],
            mark: vec![NONE; n],
            r_ptr: Vec::with_capacity(n + 1),
            r_idx: Vec::new(),
            r_slot: Vec::new(),
            in_task: vec![false; n],
            jobs: vec![(0, n)],
            scratch: vec![Vec::new()],
            flops: 0,
            above: Vec::new(),
        };
        // Every row's reach, and the column counts of L (each row adds one to every column it
        // reaches).
        let mut count = vec![1usize; n];
        chol.r_ptr.push(0);
        for k in 0..n {
            let top = chol.reach(k);
            for t in top..n {
                let i = chol.stack[t];
                count[i] += 1;
                chol.r_idx.push(i);
            }
            chol.r_ptr.push(chol.r_idx.len());
        }
        chol.l_ptr = std::iter::once(0)
            .chain(count.iter().scan(0, |s, &c| {
                *s += c;
                Some(*s)
            }))
            .collect();
        // Where each row's entries go: column i fills from its diagonal down, row by row.
        let mut next: Vec<usize> = chol.l_ptr[..n].iter().map(|p| p + 1).collect();
        chol.l_idx = vec![0; chol.l_ptr[n]];
        for k in 0..n {
            chol.l_idx[chol.l_ptr[k]] = k;
            for t in chol.r_ptr[k]..chol.r_ptr[k + 1] {
                let i = chol.r_idx[t];
                chol.l_idx[next[i]] = k;
                chol.r_slot.push(next[i]);
                next[i] += 1;
            }
        }
        chol.l_val = vec![0.0; chol.l_ptr[n]];
        chol.flops = count.iter().map(|c| c * (c - 1) / 2).sum();
        chol.plan_jobs();
        chol.factor(a)?;
        Ok(chol)
    }

    /// Rows and columns.
    pub fn n(&self) -> usize {
        self.n
    }

    /// Entries of L.
    pub fn nnz(&self) -> usize {
        self.l_idx.len()
    }

    /// A refactorisation's multiply-adds (fixed by the pattern).
    pub fn flops(&self) -> usize {
        self.flops
    }

    /// A refactorisation's cost against a solve's (in multiply-adds; a solve makes two per
    /// entry of L): whether reusing a stale factorisation, with a few more solves, is cheaper
    /// than making a new one. Fixed by the pattern, so a choice made by it is deterministic.
    pub fn factor_to_solve(&self) -> f64 {
        (self.flops + self.nnz()) as f64 / (2 * self.nnz()) as f64
    }

    /// The columns i < k where row k of L is non-zero, in an order where each comes after
    /// the ones it depends on (its descendants in the elimination tree): `stack[top..]`.
    fn reach(&mut self, k: usize) -> usize {
        let mut top = self.n;
        self.mark[k] = k;
        for p in self.c_ptr[k]..self.c_ptr[k + 1] {
            let mut i = self.c_idx[p];
            let mut len = 0;
            // Up the tree until a column already reached (or k itself).
            while self.mark[i] != k {
                self.path[len] = i;
                len += 1;
                self.mark[i] = k;
                i = self.parent[i];
            }
            // This path goes before the ones found earlier: it ends where they began.
            while len > 0 {
                len -= 1;
                top -= 1;
                self.stack[top] = self.path[len];
            }
        }
        top
    }

    /// Factorises `a` (the same pattern as before, new values).
    ///
    /// # Errors
    /// If `a` is not positive definite, or not of the pattern it was made for.
    pub fn refactor(&mut self, a: &Csc<f64>) -> Result<(), NotPositiveDefinite> {
        if a.col_ptr != self.a_ptr || a.row_idx != self.a_idx {
            *self = Self::new(a)?;
            return Ok(());
        }
        self.factor(a)
    }
}

/// The upper triangle of C = P·A·Pᵀ by column (rows sorted), the index in A of each of its
/// entries, and C's elimination tree (Liu's algorithm, with path compression).
#[allow(clippy::type_complexity)]
fn permuted(a: &Csc<f64>, perm: &[usize]) -> (Vec<usize>, Vec<usize>, Vec<usize>, Vec<usize>) {
    let n = a.n;
    let mut pinv = vec![0; n];
    for (k, &i) in perm.iter().enumerate() {
        pinv[i] = k;
    }
    let mut upper: Vec<(usize, usize, usize)> = Vec::new();
    for col in 0..n {
        for p in a.col_ptr[col]..a.col_ptr[col + 1] {
            let (i, j) = (pinv[a.row_idx[p]], pinv[col]);
            if i <= j {
                upper.push((j, i, p));
            }
        }
    }
    upper.sort_unstable();
    let mut c_ptr = vec![0; n + 1];
    for &(j, _, _) in &upper {
        c_ptr[j + 1] += 1;
    }
    for j in 0..n {
        c_ptr[j + 1] += c_ptr[j];
    }
    let c_idx: Vec<usize> = upper.iter().map(|e| e.1).collect();
    let c_from: Vec<usize> = upper.iter().map(|e| e.2).collect();
    let mut parent = vec![NONE; n];
    let mut ancestor = vec![NONE; n];
    for k in 0..n {
        for &start in &c_idx[c_ptr[k]..c_ptr[k + 1]] {
            let mut i = start;
            while i != NONE && i < k {
                let next = ancestor[i];
                ancestor[i] = k;
                if next == NONE {
                    parent[i] = k;
                }
                i = next;
            }
        }
    }
    (c_ptr, c_idx, c_from, parent)
}

/// A postorder of the forest `parent` (children in ascending order, each subtree before its
/// root): position → node.
fn postorder(parent: &[usize]) -> Vec<usize> {
    let n = parent.len();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut roots = Vec::new();
    for (i, &p) in parent.iter().enumerate() {
        if p == NONE {
            roots.push(i);
        } else {
            children[p].push(i);
        }
    }
    let mut order = Vec::with_capacity(n);
    let mut stack: Vec<(usize, usize)> = Vec::new();
    for &root in &roots {
        stack.push((root, 0));
        while let Some(&mut (node, ref mut next)) = stack.last_mut() {
            if let Some(&child) = children[node].get(*next) {
                *next += 1;
                stack.push((child, 0));
            } else {
                order.push(node);
                stack.pop();
            }
        }
    }
    order
}

/// Below this many rows a factorisation is done on one thread (the tasks would cost more).
const PARALLEL_FROM: usize = 4096;
/// Jobs a parallel factorisation is split into at most.
const JOBS: usize = 32;

impl Cholesky {
    /// The subtrees done in parallel: the largest ones of at most n/64 rows (at least 256),
    /// grouped in order into about [`JOBS`] jobs. It depends on the pattern only, not on the
    /// machine, and every row is computed alike in a task or not: the bits do not change.
    fn plan_jobs(&mut self) {
        let n = self.n;
        if n < PARALLEL_FROM {
            return;
        }
        // Subtree sizes (in postorder children come before their parents).
        let mut size = vec![1usize; n];
        for (k, &p) in self.parent.iter().enumerate() {
            if p != NONE {
                size[p] += size[k];
            }
        }
        let largest = (n / 64).max(256);
        for (task, &s) in self.in_task.iter_mut().zip(&size) {
            *task = s <= largest;
        }
        let in_tasks = self.in_task.iter().filter(|&&t| t).count();
        let per_job = in_tasks.div_ceil(JOBS).max(1);
        // Cut after a task's root once a job holds enough rows.
        let mut jobs = Vec::new();
        let (mut lo, mut held) = (0, 0);
        for (k, &s) in size.iter().enumerate() {
            let p = self.parent[k];
            let root = self.in_task[k] && (p == NONE || !self.in_task[p]);
            if root {
                held += s;
                if held >= per_job {
                    jobs.push((lo, k + 1));
                    (lo, held) = (k + 1, 0);
                }
            }
        }
        if lo < n {
            jobs.push((lo, n));
        }
        self.scratch = jobs.iter().map(|&(lo, hi)| vec![0.0; hi - lo]).collect();
        self.jobs = jobs;
    }
}

#[cfg(test)]
impl Cholesky {
    /// The same factorisation done on one thread throughout (for comparison).
    pub(crate) fn sequential(a: &Csc<f64>) -> Result<Self, NotPositiveDefinite> {
        let mut chol = Self::new(a)?;
        chol.in_task.fill(false);
        chol.jobs = vec![(0, chol.n)];
        chol.scratch = vec![Vec::new()];
        chol.factor(a)?;
        Ok(chol)
    }
}
