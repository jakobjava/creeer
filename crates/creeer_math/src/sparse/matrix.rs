//! Square sparse matrices in compressed columns, and their assembly from "stamps".
//!
//! A network's elements each add numbers at fixed positions of its matrix (a resistor between
//! nodes a and b adds g at (a,a), (b,b) and −g at (a,b), (b,a)). [`Assembly`] is made once
//! from the list of those positions: it knows the matrix's pattern, and where every
//! contribution lands. Each step then only sums the contributions into place, always in the
//! order they were listed, so the sums have the same bits every time.

use super::Scalar;

/// A square sparse matrix in compressed sparse columns (rows sorted in each column).
#[derive(Clone, Debug, PartialEq)]
pub struct Csc<T> {
    /// Rows and columns.
    pub n: usize,
    /// Column j's entries are `col_ptr[j]..col_ptr[j + 1]`.
    pub col_ptr: Vec<usize>,
    /// Their rows.
    pub row_idx: Vec<usize>,
    /// Their values.
    pub values: Vec<T>,
}

impl<T: Scalar> Csc<T> {
    /// A·x.
    ///
    /// # Panics
    /// If `x` is not of length n.
    pub fn mul_vec(&self, x: &[T]) -> Vec<T> {
        assert_eq!(x.len(), self.n, "one entry per column");
        let mut y = vec![T::ZERO; self.n];
        for (j, &xj) in x.iter().enumerate() {
            for p in self.col_ptr[j]..self.col_ptr[j + 1] {
                y[self.row_idx[p]] += self.values[p] * xj;
            }
        }
        y
    }

    /// The entry at (row, col) (zero if not stored).
    pub fn get(&self, row: usize, col: usize) -> T {
        let range = self.col_ptr[col]..self.col_ptr[col + 1];
        match self.row_idx[range.clone()].binary_search(&row) {
            Ok(k) => self.values[range.start + k],
            Err(_) => T::ZERO,
        }
    }
}

/// The pattern of a system's matrix and where each of its contributions goes.
#[derive(Clone, Debug)]
pub struct Assembly {
    n: usize,
    col_ptr: Vec<usize>,
    row_idx: Vec<usize>,
    /// For each contribution, the index of its entry.
    slot: Vec<usize>,
}

impl Assembly {
    /// The assembly of an n × n matrix whose contributions are at `positions` (row, column),
    /// in the order they will be given. Every diagonal entry is stored, stamped or not (a
    /// node nothing touches yet still has its row).
    ///
    /// # Panics
    /// If a position is outside the matrix.
    pub fn new(n: usize, positions: &[(usize, usize)]) -> Self {
        let mut entries: Vec<(usize, usize)> = positions.iter().map(|&(r, c)| (c, r)).collect();
        entries.extend((0..n).map(|i| (i, i)));
        assert!(
            entries.iter().all(|&(c, r)| c < n && r < n),
            "position outside the matrix"
        );
        entries.sort_unstable();
        entries.dedup();
        let mut col_ptr = vec![0; n + 1];
        for &(c, _) in &entries {
            col_ptr[c + 1] += 1;
        }
        for c in 0..n {
            col_ptr[c + 1] += col_ptr[c];
        }
        let row_idx: Vec<usize> = entries.iter().map(|&(_, r)| r).collect();
        let slot = positions
            .iter()
            .map(|&(r, c)| {
                let range = col_ptr[c]..col_ptr[c + 1];
                range.start + row_idx[range].binary_search(&r).expect("listed")
            })
            .collect();
        Self {
            n,
            col_ptr,
            row_idx,
            slot,
        }
    }

    /// Rows and columns.
    pub fn n(&self) -> usize {
        self.n
    }

    /// Stored entries.
    pub fn nnz(&self) -> usize {
        self.row_idx.len()
    }

    /// The matrix with `contributions` (one per position, in order) summed into place.
    ///
    /// # Panics
    /// If there is not one contribution per position.
    pub fn matrix<T: Scalar>(&self, contributions: &[T]) -> Csc<T> {
        let mut values = vec![T::ZERO; self.nnz()];
        self.fill(contributions, &mut values);
        Csc {
            n: self.n,
            col_ptr: self.col_ptr.clone(),
            row_idx: self.row_idx.clone(),
            values,
        }
    }

    /// Sums `contributions` into `values` (the matrix's values, cleared first).
    ///
    /// # Panics
    /// If there is not one contribution per position, or `values` is not one per entry.
    pub fn fill<T: Scalar>(&self, contributions: &[T], values: &mut [T]) {
        assert_eq!(
            contributions.len(),
            self.slot.len(),
            "one contribution per position"
        );
        assert_eq!(values.len(), self.nnz(), "one value per entry");
        values.fill(T::ZERO);
        for (&s, &v) in self.slot.iter().zip(contributions) {
            values[s] += v;
        }
    }

    /// An empty matrix with this pattern (to [`Self::fill`]).
    pub fn zeros<T: Scalar>(&self) -> Csc<T> {
        self.matrix(&vec![T::ZERO; self.slot.len()])
    }
}
