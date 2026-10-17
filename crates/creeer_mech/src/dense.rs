//! Small dense factorisations for a drivetrain's few unknowns (a few dozen at most: below
//! the sparse solver's overhead), in place in buffers kept from step to step.

/// Factorises the symmetric positive definite `n`×`n` row-major `a` in place into its
/// Cholesky factor L (lower triangle; the upper is left as it was). False if it is not
/// positive definite.
pub fn cholesky(a: &mut [f64], n: usize) -> bool {
    for j in 0..n {
        let mut d = a[j * n + j];
        for k in 0..j {
            d -= a[j * n + k] * a[j * n + k];
        }
        if d <= 0.0 {
            return false;
        }
        let d = d.sqrt();
        a[j * n + j] = d;
        for i in j + 1..n {
            let mut v = a[i * n + j];
            for k in 0..j {
                v -= a[i * n + k] * a[j * n + k];
            }
            a[i * n + j] = v / d;
        }
    }
    true
}

/// Solves L·Lᵀ·x = b in place with the factor of [`cholesky`].
pub fn cholesky_solve(l: &[f64], n: usize, b: &mut [f64]) {
    for i in 0..n {
        let mut v = b[i];
        for k in 0..i {
            v -= l[i * n + k] * b[k];
        }
        b[i] = v / l[i * n + i];
    }
    for i in (0..n).rev() {
        let mut v = b[i];
        for k in i + 1..n {
            v -= l[k * n + i] * b[k];
        }
        b[i] = v / l[i * n + i];
    }
}

/// Factorises the `n`×`n` row-major `a` in place with full pivoting, its row and column
/// order into `rows` and `cols`, stopping where the largest remaining pivot is below
/// `tolerance` times the first: what is left depends on what came before (a redundant
/// constraint). Returns how many pivots were kept.
pub fn factor_full(
    a: &mut [f64],
    n: usize,
    tolerance: f64,
    rows: &mut Vec<usize>,
    cols: &mut Vec<usize>,
) -> usize {
    rows.clear();
    rows.extend(0..n);
    cols.clear();
    cols.extend(0..n);
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
            return j;
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
    n
}

/// Solves with the factors of [`factor_full`] (`rank` pivots kept) into `x`, `z` its
/// scratch: the unknowns of the dependent part are zero (their rows are satisfied by the
/// others' when the system is consistent).
pub fn solve_full(
    lu: &[f64],
    (rows, cols, rank): (&[usize], &[usize], usize),
    b: &[f64],
    x: &mut [f64],
    z: &mut Vec<f64>,
) {
    let n = rows.len();
    z.clear();
    z.extend(rows.iter().map(|&p| b[p]));
    for i in 0..rank {
        for k in 0..i {
            z[i] -= lu[i * n + k] * z[k];
        }
    }
    for i in (0..rank).rev() {
        for k in i + 1..rank {
            z[i] -= lu[i * n + k] * z[k];
        }
        z[i] /= lu[i * n + i];
    }
    x.fill(0.0);
    for j in 0..rank {
        x[cols[j]] = z[j];
    }
}
