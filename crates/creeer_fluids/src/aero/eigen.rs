//! A symmetric 3×3 matrix's positive part: its eigenvectors with the negative eigenvalues
//! set to nought (Jacobi's rotations; three to five sweeps reach rounding).

use creeer_math::glam::{DMat3, DVec3};

/// `m` (symmetric) with its negative eigenvalues dropped: the nearest positive semidefinite
/// matrix.
#[allow(clippy::needless_range_loop)] // Jacobi's rotations read best with indices
pub fn positive_part(m: DMat3) -> DMat3 {
    let mut a = m.to_cols_array_2d();
    let mut v = DMat3::IDENTITY.to_cols_array_2d();
    let scale: f64 = a.iter().flatten().map(|x| x.abs()).sum();
    if scale == 0.0 {
        return DMat3::ZERO;
    }
    for _ in 0..12 {
        let off = a[0][1].abs() + a[0][2].abs() + a[1][2].abs();
        if off <= 1e-15 * scale {
            break;
        }
        for (p, q) in [(0, 1), (0, 2), (1, 2)] {
            if a[p][q] == 0.0 {
                continue;
            }
            let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
            let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
            let t = if theta == 0.0 { 1.0 } else { t };
            let c = 1.0 / (t * t + 1.0).sqrt();
            let s = t * c;
            // A ← Jᵀ·A·J, V ← V·J for the rotation J in the (p, q) plane.
            for k in 0..3 {
                let (akp, akq) = (a[k][p], a[k][q]);
                a[k][p] = c * akp - s * akq;
                a[k][q] = s * akp + c * akq;
            }
            for k in 0..3 {
                let (apk, aqk) = (a[p][k], a[q][k]);
                a[p][k] = c * apk - s * aqk;
                a[q][k] = s * apk + c * aqk;
            }
            for k in 0..3 {
                let (vp, vq) = (v[p][k], v[q][k]);
                v[p][k] = c * vp - s * vq;
                v[q][k] = s * vp + c * vq;
            }
        }
    }
    // The columns of V (as stored: v[col][row]) are the eigenvectors.
    let vectors = DMat3::from_cols_array_2d(&v);
    let mut out = DMat3::ZERO;
    for k in 0..3 {
        let lambda = a[k][k];
        if lambda > 0.0 {
            let e: DVec3 = vectors.col(k);
            out += DMat3::from_cols(e * e.x, e * e.y, e * e.z) * lambda;
        }
    }
    out
}
