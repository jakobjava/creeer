//! Exact finite-element reference for the spike truss (direct stiffness method).

use super::*;

/// Exact linear-elastic static midspan sag of the *same discrete truss*, solved with the
/// direct stiffness method (finite elements): assemble K, solve K·u = f.
///
/// This is the reference XPBD must converge to. Unlike beam theory it has no modelling
/// approximation (discrete bays, real diagonals, real supports) and no iteration error.
/// Dense Cholesky is fine for one bridge (300 unknowns); the production quasi-static solver
/// (F2) will use a sparse factorisation.
pub fn linear_static_sag(spec: BridgeSpec) -> f64 {
    let truss = Truss::box_bridge(DVec3::ZERO, spec);
    let u = linear_static_displacement(spec);
    truss
        .midspan_nodes
        .iter()
        .map(|&node| -u[node].y)
        .sum::<f64>()
        / 4.0
}

/// Exact linear-elastic static displacement of every node of the reference truss under
/// self-weight (dense direct stiffness method, see [`linear_static_sag`]).
pub fn linear_static_displacement(spec: BridgeSpec) -> Vec<DVec3> {
    let truss = Truss::box_bridge(DVec3::ZERO, spec);
    let n = 3 * truss.node_count();
    let ea = spec.material.youngs_modulus * spec.member_area;

    // Stiffness of a bar along unit vector e: (EA/L)·[e·eᵀ, −e·eᵀ; −e·eᵀ, e·eᵀ].
    let mut k = vec![0.0; n * n];
    for beam in &truss.beams {
        let e = (truss.rest[beam.a] - truss.rest[beam.b]) / beam.rest_length;
        let stiffness = ea / beam.rest_length;
        for (i, ei) in e.to_array().into_iter().enumerate() {
            for (j, ej) in e.to_array().into_iter().enumerate() {
                let kij = stiffness * ei * ej;
                for (row_node, col_node, sign) in [
                    (beam.a, beam.a, 1.0),
                    (beam.b, beam.b, 1.0),
                    (beam.a, beam.b, -1.0),
                    (beam.b, beam.a, -1.0),
                ] {
                    k[(3 * row_node + i) * n + 3 * col_node + j] += sign * kij;
                }
            }
        }
    }

    // Gravity on each node's own mass (zero inverse mass on fixed axes means "support").
    let mut f = vec![0.0; n];
    let mut fixed = vec![false; n];
    for (node, w) in truss.inv_mass.iter().enumerate() {
        for (axis, inv) in w.to_array().into_iter().enumerate() {
            fixed[3 * node + axis] = inv == 0.0;
        }
    }
    let half_masses = truss.beams.iter().map(|b| {
        let m = spec.material.density * spec.member_area * b.rest_length / 2.0;
        (b.a, b.b, m)
    });
    for (a, b, m) in half_masses {
        f[3 * a + 1] += m * GRAVITY.y;
        f[3 * b + 1] += m * GRAVITY.y;
    }

    // Supports: replace fixed rows/columns by identity (u = 0 there).
    for d in (0..n).filter(|&d| fixed[d]) {
        for j in 0..n {
            k[d * n + j] = 0.0;
            k[j * n + d] = 0.0;
        }
        k[d * n + d] = 1.0;
        f[d] = 0.0;
    }

    let u = cholesky_solve(&mut k, &mut f, n);
    u.as_chunks::<3>()
        .0
        .iter()
        .map(|&[x, y, z]| DVec3::new(x, y, z))
        .collect()
}

/// Solves K·u = f in place for a symmetric positive-definite K (row-major n×n).
///
/// # Panics
/// If K is not positive definite (e.g. the structure is a mechanism).
fn cholesky_solve(k: &mut [f64], f: &mut [f64], n: usize) -> Vec<f64> {
    // Factor K = L·Lᵀ, storing L in the lower triangle.
    for j in 0..n {
        let mut diag = k[j * n + j];
        for p in 0..j {
            diag -= k[j * n + p] * k[j * n + p];
        }
        assert!(
            diag > 0.0,
            "stiffness matrix not positive definite at dof {j}"
        );
        let ljj = diag.sqrt();
        k[j * n + j] = ljj;
        for i in j + 1..n {
            let mut sum = k[i * n + j];
            for p in 0..j {
                sum -= k[i * n + p] * k[j * n + p];
            }
            k[i * n + j] = sum / ljj;
        }
    }
    // Forward substitution L·y = f, then back substitution Lᵀ·u = y.
    for i in 0..n {
        let mut sum = f[i];
        for p in 0..i {
            sum -= k[i * n + p] * f[p];
        }
        f[i] = sum / k[i * n + i];
    }
    for i in (0..n).rev() {
        let mut sum = f[i];
        for p in i + 1..n {
            sum -= k[p * n + i] * f[p];
        }
        f[i] = sum / k[i * n + i];
    }
    f.to_vec()
}
