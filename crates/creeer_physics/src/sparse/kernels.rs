//! The 3×3 block kernels of the factorisation and the substitutions, written lane by lane so
//! that the compiler can put a block's three rows in one SIMD register.
//!
//! **The same bits on every platform and CPU.** Each lane does exactly the operations
//! glam's `DMat3 * DVec3` does for that row, in its order: (c₀·x + c₁·y) + c₂·z, then the
//! subtraction. Rust never contracts a multiply and an add into a fused one, nor reorders
//! floating-point sums, so a lane computed in a 256-bit AVX2 register, a 128-bit NEON one or
//! a scalar one gives the same result (tested against the glam expressions bit for bit,
//! `kernel_tests.rs`). What differs between CPUs is only how many lanes a register holds:
//! the hot loops are compiled twice on x86-64, for the baseline and for AVX2, and the AVX2
//! copy is chosen at run time (`multiversion`, without FMA); aarch64 has NEON throughout.
//!
//! A block is read as three overlapping four-lane columns of its nine contiguous values
//! (the fourth lane of the first two holds the next column's first value, of the third a
//! zero): one load a column, no padding in memory. The fourth lane's result is never used.
//! The backward substitution reads the same blocks for mᵀ·v, their rows gathered into lanes.

use creeer_math::glam::{DMat3, DVec3};
use multiversion::multiversion;

/// Four lanes: a block's column (three rows and a spare lane), or a vector.
type Lanes = [f64; 4];

#[inline(always)]
fn splat(v: f64) -> Lanes {
    [v; 4]
}

#[inline(always)]
fn mul(a: Lanes, b: Lanes) -> Lanes {
    [a[0] * b[0], a[1] * b[1], a[2] * b[2], a[3] * b[3]]
}

#[inline(always)]
fn add(a: Lanes, b: Lanes) -> Lanes {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2], a[3] + b[3]]
}

#[inline(always)]
fn sub(a: Lanes, b: Lanes) -> Lanes {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2], a[3] - b[3]]
}

#[inline(always)]
fn lanes(v: DVec3) -> Lanes {
    [v.x, v.y, v.z, 0.0]
}

#[inline(always)]
fn vector(l: Lanes) -> DVec3 {
    DVec3::new(l[0], l[1], l[2])
}

/// A block's three columns.
#[inline(always)]
fn columns(m: &DMat3) -> [Lanes; 3] {
    let a: &[f64; 9] = m.as_ref();
    [
        [a[0], a[1], a[2], a[3]],
        [a[3], a[4], a[5], a[6]],
        [a[6], a[7], a[8], 0.0],
    ]
}

/// m·v as glam computes it: (c₀·x + c₁·y) + c₂·z.
#[inline(always)]
fn mat_vec(m: &DMat3, v: Lanes) -> Lanes {
    let [c0, c1, c2] = columns(m);
    add(
        add(mul(c0, splat(v[0])), mul(c1, splat(v[1]))),
        mul(c2, splat(v[2])),
    )
}

/// mᵀ·v as glam's `m.transpose() * v` computes it: the rows of m as the columns.
#[inline(always)]
fn mat_t_vec(m: &DMat3, v: Lanes) -> Lanes {
    let a: &[f64; 9] = m.as_ref();
    let (r0, r1, r2) = (
        [a[0], a[3], a[6], 0.0],
        [a[1], a[4], a[7], 0.0],
        [a[2], a[5], a[8], 0.0],
    );
    add(
        add(mul(r0, splat(v[0])), mul(r1, splat(v[1]))),
        mul(r2, splat(v[2])),
    )
}

/// x[k_e − lo] −= m_eᵀ·xj for every entry: a row's push in the backward substitution, from
/// the same blocks the forward one reads.
#[multiversion(targets("x86_64+avx+avx2"))]
pub(super) fn row_push_t(xj: DVec3, blocks: &[DMat3], cols: &[u32], x: &mut [DVec3], lo: usize) {
    let xj = lanes(xj);
    for (m, &k) in blocks.iter().zip(cols) {
        let at = &mut x[k as usize - lo];
        *at = vector(sub(lanes(*at), mat_t_vec(m, xj)));
    }
}

/// b − Σ m_e·x[k_e − lo] over the entries in order: a row of the forward substitution.
#[multiversion(targets("x86_64+avx+avx2"))]
pub(super) fn row_sum(b: DVec3, blocks: &[DMat3], cols: &[u32], x: &[DVec3], lo: usize) -> DVec3 {
    let mut acc = lanes(b);
    for (m, &k) in blocks.iter().zip(cols) {
        acc = sub(acc, mat_vec(m, lanes(x[k as usize - lo])));
    }
    vector(acc)
}

/// target −= a·bᵀ, as glam's `target -= a * b.transpose()` (column c of a·bᵀ is a·(row c of
/// b), each a matrix-vector product as above): the factorisation's block update.
#[inline(always)]
pub(super) fn sub_mul_transpose(target: &mut DMat3, a: &DMat3, b: &DMat3) {
    let bt: &[f64; 9] = b.as_ref();
    let t: &mut [f64; 9] = target.as_mut();
    let column = |c: usize| mat_vec(a, [bt[c], bt[3 + c], bt[6 + c], 0.0]);
    for c in 0..3 {
        let p = column(c);
        let now = [t[3 * c], t[3 * c + 1], t[3 * c + 2], 0.0];
        let new = sub(now, p);
        t[3 * c..3 * c + 3].copy_from_slice(&new[..3]);
    }
}
