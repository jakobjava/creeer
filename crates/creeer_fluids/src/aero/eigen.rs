//! A symmetric 3×3 matrix's positive part: its eigenvectors with the negative eigenvalues
//! set to nought. The eigenvalues in closed form (Smith's trigonometric solution of the
//! characteristic cubic, CACM 4, 1961); the one eigenvector it needs, of a simple
//! eigenvalue, as the longest cross product of two rows of A − λ·I.

use creeer_math::det;
use creeer_math::glam::{DMat3, DVec3};

/// `m` (symmetric) with its negative eigenvalues dropped: the nearest positive semidefinite
/// matrix.
pub fn positive_part(m: DMat3) -> DMat3 {
    let (a, b, c) = (m.x_axis.x, m.y_axis.y, m.z_axis.z);
    let (ab, ac, bc) = (m.y_axis.x, m.z_axis.x, m.z_axis.y);
    // Already semidefinite (all its principal minors non-negative): itself.
    let minors = a * b >= ab * ab && a * c >= ac * ac && b * c >= bc * bc;
    if a >= 0.0 && b >= 0.0 && c >= 0.0 && minors && m.determinant() >= 0.0 {
        return m;
    }
    let [high, middle, low] = eigenvalues(m);
    if high <= 0.0 {
        return DMat3::ZERO;
    }
    if middle >= 0.0 {
        // One negative: its direction taken out.
        let e = vector(m, low);
        m - outer(e) * low
    } else {
        // One positive: only its direction kept.
        outer(vector(m, high)) * high
    }
}

fn outer(e: DVec3) -> DMat3 {
    DMat3::from_cols(e * e.x, e * e.y, e * e.z)
}

/// The eigenvalues, largest first.
fn eigenvalues(m: DMat3) -> [f64; 3] {
    let (a, b, c) = (m.x_axis.x, m.y_axis.y, m.z_axis.z);
    let (ab, ac, bc) = (m.y_axis.x, m.z_axis.x, m.z_axis.y);
    let off = ab * ab + ac * ac + bc * bc;
    let q = (a + b + c) / 3.0;
    let spread = (a - q) * (a - q) + (b - q) * (b - q) + (c - q) * (c - q) + 2.0 * off;
    if spread <= 0.0 {
        return [q; 3];
    }
    let p = (spread / 6.0).sqrt();
    let r = ((m - DMat3::IDENTITY * q) * (1.0 / p)).determinant() / 2.0;
    let phi = det::acos(r.clamp(-1.0, 1.0)) / 3.0;
    let high = q + 2.0 * p * det::cos(phi);
    let low = q + 2.0 * p * det::cos(phi + 2.0 * std::f64::consts::PI / 3.0);
    [high, 3.0 * q - high - low, low]
}

/// A unit eigenvector of the simple eigenvalue `lambda`: A − λ·I has rank two, and its rows
/// span the plane the vector is square to.
fn vector(m: DMat3, lambda: f64) -> DVec3 {
    let s = m - DMat3::IDENTITY * lambda;
    // (Symmetric: its columns are its rows.)
    let (r0, r1, r2) = (s.x_axis, s.y_axis, s.z_axis);
    let mut best = r0.cross(r1);
    for c in [r0.cross(r2), r1.cross(r2)] {
        if c.length_squared() > best.length_squared() {
            best = c;
        }
    }
    best.normalize_or_zero()
}
