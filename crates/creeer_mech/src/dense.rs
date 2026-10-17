//! A small dense LU with partial pivoting, for a drivetrain's few unknowns (a few dozen at
//! most: below the sparse solver's overhead).

/// Factorises the `n`×`n` row-major matrix `a` in place; returns its pivots, or `None` if it
/// is singular.
pub fn factor(a: &mut [f64], n: usize) -> Option<Vec<usize>> {
    let mut pivots: Vec<usize> = (0..n).collect();
    for j in 0..n {
        let p = (j..n).max_by(|&x, &y| a[x * n + j].abs().total_cmp(&a[y * n + j].abs()))?;
        if a[p * n + j] == 0.0 {
            return None;
        }
        if p != j {
            for k in 0..n {
                a.swap(p * n + k, j * n + k);
            }
            pivots.swap(p, j);
        }
        let d = a[j * n + j];
        for i in j + 1..n {
            let f = a[i * n + j] / d;
            a[i * n + j] = f;
            if f != 0.0 {
                for k in j + 1..n {
                    a[i * n + k] -= f * a[j * n + k];
                }
            }
        }
    }
    Some(pivots)
}

/// Solves with the factors of [`factor`]: `b` in, the solution out.
pub fn solve(lu: &[f64], pivots: &[usize], b: &[f64]) -> Vec<f64> {
    let n = pivots.len();
    let mut x: Vec<f64> = pivots.iter().map(|&p| b[p]).collect();
    for i in 0..n {
        for k in 0..i {
            x[i] -= lu[i * n + k] * x[k];
        }
    }
    for i in (0..n).rev() {
        for k in i + 1..n {
            x[i] -= lu[i * n + k] * x[k];
        }
        x[i] /= lu[i * n + i];
    }
    x
}

/// The factors of [`factor_full`]: rows and columns as pivoted, and how many pivots were
/// kept before the rest fell below the tolerance (dependent rows).
pub struct Full {
    rows: Vec<usize>,
    cols: Vec<usize>,
    pub rank: usize,
}

/// Factorises the `n`×`n` row-major `a` in place with full pivoting, stopping where the
/// largest remaining pivot is below `tolerance` times the first: what is left is dependent
/// on what came before (a redundant constraint).
pub fn factor_full(a: &mut [f64], n: usize, tolerance: f64) -> Full {
    let mut rows: Vec<usize> = (0..n).collect();
    let mut cols: Vec<usize> = (0..n).collect();
    let mut first = 0.0;
    for j in 0..n {
        let (mut pi, mut pk, mut best) = (j, j, 0.0);
        for i in j..n {
            for k in j..n {
                if a[i * n + k].abs() > best {
                    (pi, pk, best) = (i, k, a[i * n + k].abs());
                }
            }
        }
        if j == 0 {
            first = best;
        }
        if best == 0.0 || best <= tolerance * first {
            return Full {
                rows,
                cols,
                rank: j,
            };
        }
        if pi != j {
            for k in 0..n {
                a.swap(pi * n + k, j * n + k);
            }
            rows.swap(pi, j);
        }
        if pk != j {
            for i in 0..n {
                a.swap(i * n + pk, i * n + j);
            }
            cols.swap(pk, j);
        }
        let d = a[j * n + j];
        for i in j + 1..n {
            let f = a[i * n + j] / d;
            a[i * n + j] = f;
            if f != 0.0 {
                for k in j + 1..n {
                    a[i * n + k] -= f * a[j * n + k];
                }
            }
        }
    }
    Full {
        rows,
        cols,
        rank: n,
    }
}

/// Solves with the factors of [`factor_full`]: the unknowns of the dependent part are zero
/// (their rows are satisfied by the others' when the system is consistent).
pub fn solve_full(lu: &[f64], full: &Full, b: &[f64]) -> Vec<f64> {
    let n = full.rows.len();
    let r = full.rank;
    let mut z: Vec<f64> = full.rows.iter().map(|&p| b[p]).collect();
    for i in 0..r {
        for k in 0..i {
            z[i] -= lu[i * n + k] * z[k];
        }
    }
    for i in (0..r).rev() {
        for k in i + 1..r {
            z[i] -= lu[i * n + k] * z[k];
        }
        z[i] /= lu[i * n + i];
    }
    let mut x = vec![0.0; n];
    for j in 0..r {
        x[full.cols[j]] = z[j];
    }
    x
}
