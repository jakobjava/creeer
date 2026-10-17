//! A small dense solve for the few unknowns of an equilibrium's or a reactor's Newton step.

/// Solves the dense `n`×`n` system `a`·x = `b` in place (partial pivoting): `b` becomes x.
/// False if singular.
pub(crate) fn solve(a: &mut [f64], b: &mut [f64], n: usize) -> bool {
    for j in 0..n {
        let p = (j..n)
            .max_by(|&x, &y| a[x * n + j].abs().total_cmp(&a[y * n + j].abs()))
            .unwrap_or(j);
        if a[p * n + j].abs() < 1e-300 {
            return false;
        }
        if p != j {
            for k in 0..n {
                a.swap(p * n + k, j * n + k);
            }
            b.swap(p, j);
        }
        for i in j + 1..n {
            let f = a[i * n + j] / a[j * n + j];
            if f != 0.0 {
                for k in j..n {
                    a[i * n + k] -= f * a[j * n + k];
                }
                b[i] -= f * b[j];
            }
        }
    }
    for j in (0..n).rev() {
        let s: f64 = (j + 1..n).map(|k| a[j * n + k] * b[k]).sum();
        b[j] = (b[j] - s) / a[j * n + j];
    }
    true
}
