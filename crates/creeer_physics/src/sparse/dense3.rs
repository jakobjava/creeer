//! 3×3 dense kernels for the block factorisation.

use creeer_math::glam::{DMat3, DVec3};

/// Cholesky factor (lower triangular) of a symmetric 3×3 block, or `None` if the block is not
/// positive definite. Only the lower triangle of `a` is read.
pub(super) fn cholesky(a: DMat3) -> Option<DMat3> {
    // glam is column-major: a.col(c)[r] is row r, column c.
    let at = |r: usize, c: usize| a.col(c)[r];
    let pivot = |value: f64| (value > 0.0 && value.is_finite()).then(|| value.sqrt());
    let l00 = pivot(at(0, 0))?;
    let l10 = at(1, 0) / l00;
    let l20 = at(2, 0) / l00;
    let l11 = pivot(at(1, 1) - l10 * l10)?;
    let l21 = (at(2, 1) - l20 * l10) / l11;
    let l22 = pivot(at(2, 2) - l20 * l20 - l21 * l21)?;
    Some(DMat3::from_cols(
        DVec3::new(l00, l10, l20),
        DVec3::new(0.0, l11, l21),
        DVec3::new(0.0, 0.0, l22),
    ))
}

/// Inverse of a lower-triangular 3×3 block with non-zero diagonal.
pub(super) fn lower_inverse(l: DMat3) -> DMat3 {
    let at = |r: usize, c: usize| l.col(c)[r];
    let i00 = 1.0 / at(0, 0);
    let i11 = 1.0 / at(1, 1);
    let i22 = 1.0 / at(2, 2);
    let i10 = -at(1, 0) * i00 * i11;
    let i21 = -at(2, 1) * i11 * i22;
    let i20 = -(at(2, 0) * i00 + at(2, 1) * i10) * i22;
    DMat3::from_cols(
        DVec3::new(i00, i10, i20),
        DVec3::new(0.0, i11, i21),
        DVec3::new(0.0, 0.0, i22),
    )
}
