//! The SIMD kernels against the glam expressions they replace, bit for bit (`kernels.rs`).

use super::kernels;
use creeer_math::glam::{DMat3, DVec3};

/// Values of every magnitude and sign, with exact zeros and signed zeros among them
/// (a supported node's fixed axes give zeros, and a zero's sign must come out as glam's).
fn values(seed: u64, count: usize) -> Vec<f64> {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    (0..count)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            match state % 11 {
                0 => 0.0,
                1 => -0.0,
                _ => {
                    #[allow(clippy::cast_precision_loss)]
                    let unit = (state >> 11) as f64 / (1u64 << 53) as f64 - 0.5;
                    let scale = [1e-9, 1e-3, 1.0, 1e3, 1e9][(state % 5) as usize];
                    unit * scale
                }
            }
        })
        .collect()
}

fn matrix(v: &[f64]) -> DMat3 {
    DMat3::from_cols_slice(v)
}

fn vector(v: &[f64]) -> DVec3 {
    DVec3::from_slice(v)
}

fn same(a: DVec3, b: DVec3) -> bool {
    a.to_array()
        .iter()
        .zip(b.to_array())
        .all(|(x, y)| x.to_bits() == y.to_bits())
}

#[test]
fn a_row_sum_is_glam_bit_for_bit() {
    for seed in 0..200_usize {
        let v = values(seed as u64, 400);
        let count = 1 + seed % 20;
        let blocks: Vec<DMat3> = (0..count).map(|e| matrix(&v[9 * e..])).collect();
        let x: Vec<DVec3> = (0..10).map(|i| vector(&v[200 + 3 * i..])).collect();
        let cols: Vec<u32> = (0..count)
            .map(|e| u32::try_from((e * 7 + 3) % 10).unwrap())
            .collect();
        let b = vector(&v[390..]);
        let mut expected = b;
        for (m, &k) in blocks.iter().zip(&cols) {
            expected -= *m * x[k as usize];
        }
        assert!(same(kernels::row_sum(b, &blocks, &cols, &x, 0), expected));
    }
}

#[test]
fn a_row_push_is_glam_bit_for_bit() {
    for seed in 0..200_usize {
        let v = values(seed as u64 + 1000, 400);
        let count = 1 + seed % 20;
        let blocks: Vec<DMat3> = (0..count).map(|e| matrix(&v[9 * e..])).collect();
        let x: Vec<DVec3> = (0..10).map(|i| vector(&v[200 + 3 * i..])).collect();
        // Distinct columns, as in a row of L.
        let cols: Vec<u32> = (0..count.min(10))
            .map(|e| u32::try_from(e).unwrap())
            .collect();
        let xj = vector(&v[390..]);
        let mut expected = x.clone();
        for (m, &k) in blocks.iter().zip(&cols) {
            expected[k as usize] -= m.transpose() * xj;
        }
        let mut got = x.clone();
        kernels::row_push_t(xj, &blocks, &cols, &mut got, 0);
        assert!(got.iter().zip(&expected).all(|(a, b)| same(*a, *b)));
    }
}

#[test]
fn the_block_update_is_glam_bit_for_bit() {
    for seed in 0..500 {
        let v = values(seed + 5000, 27);
        let (a, b, t) = (matrix(&v), matrix(&v[9..]), matrix(&v[18..]));
        let mut expected = t;
        expected -= a * b.transpose();
        let mut got = t;
        kernels::sub_mul_transpose(&mut got, &a, &b);
        let bits = |m: DMat3| m.to_cols_array().map(f64::to_bits);
        assert_eq!(bits(got), bits(expected));
    }
}
