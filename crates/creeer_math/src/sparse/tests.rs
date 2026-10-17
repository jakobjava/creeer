//! The solvers solve, pivot where they must, notice singular and indefinite matrices, refactor
//! to the very bits of a fresh factorisation, and keep fill low.

use super::*;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        #[allow(clippy::cast_precision_loss)]
        let u = (self.0 >> 11) as f64 / (1u64 << 53) as f64;
        u * 2.0 - 1.0
    }

    fn index(&mut self, n: usize) -> usize {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let i = ((self.next() + 1.0) * 0.5 * n as f64) as usize;
        i.min(n - 1)
    }
}

/// A random sparse matrix: `per_row` off-diagonal entries a row, the diagonal `dominance`
/// times the row's sum (below 1: pivoting needed).
fn random(n: usize, per_row: usize, dominance: f64, seed: u64) -> Csc<f64> {
    let mut rng = Rng(seed);
    let mut positions = Vec::new();
    let mut values = Vec::new();
    for r in 0..n {
        let mut sum = 0.0;
        for _ in 0..per_row {
            let c = rng.index(n);
            if c != r {
                let v = rng.next();
                positions.push((r, c));
                values.push(v);
                sum += v.abs();
            }
        }
        positions.push((r, r));
        let sign = if rng.next() < 0.0 { -1.0 } else { 1.0 };
        values.push(dominance * sum.max(0.1) * sign);
    }
    Assembly::new(n, &positions).matrix(&values)
}

fn residual<T: Scalar>(a: &Csc<T>, x: &[T], b: &[T]) -> f64 {
    let ax = a.mul_vec(x);
    ax.iter()
        .zip(b)
        .map(|(p, q)| (*p - *q).magnitude())
        .fold(0.0, f64::max)
}

fn bits(x: &[f64]) -> Vec<u64> {
    x.iter().map(|v| v.to_bits()).collect()
}

#[test]
fn lu_solves_sparse_systems_with_and_without_pivoting() {
    for (dominance, seed) in [(2.0, 1), (0.3, 2), (0.05, 3)] {
        let a = random(400, 4, dominance, seed);
        let mut rng = Rng(seed + 10);
        let b: Vec<f64> = (0..400).map(|_| rng.next()).collect();
        let mut lu = Lu::new(&a).unwrap();
        let mut x = b.clone();
        lu.solve(&mut x);
        let r = residual(&a, &x, &b);
        assert!(r < 1e-10, "dominance {dominance}: residual {r}");
    }
}

#[test]
fn lu_pivots_past_a_zero_diagonal() {
    // A conductance and an ideal voltage source (MNA): [[g, 1], [1, 0]]·[v, i] = [0, 5].
    let a = Assembly::new(2, &[(0, 0), (0, 1), (1, 0)]).matrix(&[0.5, 1.0, 1.0]);
    let mut x = vec![0.0, 5.0];
    Lu::new(&a).unwrap().solve(&mut x);
    assert!(
        (x[0] - 5.0).abs() < 1e-15 && (x[1] + 2.5).abs() < 1e-15,
        "{x:?}"
    );
}

#[test]
fn singular_and_indefinite_matrices_are_reported() {
    // Two nodes joined by a conductance and to nothing else: no reference, singular.
    let positions = [(0, 0), (1, 1), (0, 1), (1, 0)];
    let a = Assembly::new(2, &positions).matrix(&[1.0, 1.0, -1.0, -1.0]);
    assert!(Lu::new(&a).is_err());
    assert!(Cholesky::new(&a).is_err());
    let indefinite = Assembly::new(2, &positions).matrix(&[1.0, -1.0, 0.0, 0.0]);
    assert!(Cholesky::new(&indefinite).is_err());
}

#[test]
fn a_refactorisation_gives_the_bits_of_a_fresh_one() {
    let a = random(300, 3, 1.5, 7);
    let mut changed = a.clone();
    for (k, v) in changed.values.iter_mut().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let s = 1.0 + 0.01 * (k % 7) as f64;
        *v *= s;
    }
    let mut lu = Lu::new(&a).unwrap();
    assert!(lu.refactor(&changed).unwrap(), "the pivots still serve");
    let mut fresh = Lu::new(&changed).unwrap();
    let b: Vec<f64> = (0..300).map(|i| f64::from(i) * 0.01 - 1.0).collect();
    let (mut x1, mut x2) = (b.clone(), b.clone());
    lu.solve(&mut x1);
    fresh.solve(&mut x2);
    assert_eq!(bits(&x1), bits(&x2));
    // A change that ruins a pivot: a fresh factorisation, still a solution.
    let mut ruined = changed.clone();
    let column = ruined.col_ptr[5]..ruined.col_ptr[6];
    let d = column.start + ruined.row_idx[column].binary_search(&5).unwrap();
    ruined.values[d] = 0.0;
    let reused = lu.refactor(&ruined).unwrap();
    assert!(!reused);
    let mut x = b.clone();
    lu.solve(&mut x);
    assert!(residual(&ruined, &x, &b) < 1e-10);
}

#[test]
fn complex_systems_solve() {
    // An RL ladder in phasors: series R + jX, shunt capacitance to ground, 20 sections, fed
    // through a unit source admittance.
    let n = 20;
    let (z, y_shunt) = (Complex::new(0.5, 0.8), Complex::new(0.0, 0.05));
    let y = Complex::ONE / z;
    let mut positions = Vec::new();
    let mut values = Vec::new();
    for i in 0..n {
        let mut diagonal = y_shunt + if i > 0 { y } else { Complex::ONE };
        if i + 1 < n {
            diagonal += y;
            positions.extend([(i, i + 1), (i + 1, i)]);
            values.extend([-y, -y]);
        }
        positions.push((i, i));
        values.push(diagonal);
    }
    let a = Assembly::new(n, &positions).matrix(&values);
    let mut b = vec![Complex::ZERO; n];
    b[0] = Complex::new(230.0, 0.0);
    let rhs = b.clone();
    Lu::new(&a).unwrap().solve(&mut b);
    assert!(residual(&a, &b, &rhs) < 1e-10);
}

/// A 2D grid of conductances, each node also to ground: a heat-conduction-like matrix.
fn grid(side: usize, ground: f64) -> Csc<f64> {
    let id = |x: usize, y: usize| y * side + x;
    let (mut positions, mut values) = (Vec::new(), Vec::new());
    for y in 0..side {
        for x in 0..side {
            let i = id(x, y);
            positions.push((i, i));
            values.push(ground);
            for (dx, dy) in [(1, 0), (0, 1)] {
                if x + dx < side && y + dy < side {
                    let j = id(x + dx, y + dy);
                    positions.extend([(i, i), (j, j), (i, j), (j, i)]);
                    values.extend([1.0, 1.0, -1.0, -1.0]);
                }
            }
        }
    }
    Assembly::new(side * side, &positions).matrix(&values)
}

#[test]
fn cholesky_solves_and_refactors_and_agrees_with_lu() {
    let a = grid(30, 0.01);
    let b: Vec<f64> = (0..900).map(|i| f64::from(i % 17) - 8.0).collect();
    let mut chol = Cholesky::new(&a).unwrap();
    let mut x = b.clone();
    chol.solve(&mut x);
    assert!(residual(&a, &x, &b) < 1e-9);
    let mut y = b.clone();
    Lu::new(&a).unwrap().solve(&mut y);
    let diff = x
        .iter()
        .zip(&y)
        .map(|(p, q)| (p - q).abs())
        .fold(0.0, f64::max);
    assert!(diff < 1e-8, "{diff}");
    let mut stiffer = a.clone();
    for v in &mut stiffer.values {
        *v *= 2.0;
    }
    chol.refactor(&stiffer).unwrap();
    let mut z = b.clone();
    chol.solve(&mut z);
    let half = x
        .iter()
        .zip(&z)
        .map(|(p, q)| (p - 2.0 * q).abs())
        .fold(0.0, f64::max);
    assert!(half < 1e-8, "{half}");
}

#[test]
fn minimum_degree_is_a_permutation_and_keeps_fill_low() {
    let a = grid(40, 0.1);
    let order = minimum_degree(a.n, &a.col_ptr, &a.row_idx);
    let mut sorted = order.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, (0..a.n).collect::<Vec<_>>());
    // The natural order fills the whole band (40 entries a row: 64k); minimum degree much
    // less.
    let nnz = Cholesky::new(&a).unwrap().nnz();
    assert!(nnz < 25_000, "{nnz} entries in L");
}

#[test]
fn results_do_not_depend_on_the_run() {
    let a = random(500, 4, 0.5, 11);
    let b: Vec<f64> = (0..500).map(|i| f64::from(i).sqrt()).collect();
    let solve = || {
        let mut x = b.clone();
        Lu::new(&a).unwrap().solve(&mut x);
        bits(&x)
    };
    assert_eq!(solve(), solve());
}

/// Machines hanging off a floor: many independent subtrees (the parallel case).
fn machines(count: usize, parts: usize) -> Csc<f64> {
    let (mut positions, mut values) = (Vec::new(), Vec::new());
    let mut conduct = |i: usize, j: usize, g: f64| {
        positions.extend([(i, i), (j, j), (i, j), (j, i)]);
        values.extend([g, g, -g, -g]);
    };
    for m in 0..count {
        if m + 1 < count {
            conduct(m, m + 1, 40.0);
        }
        let frame = count + m * (parts + 1);
        conduct(m, frame, 10.0);
        for k in 1..=parts {
            conduct(frame, frame + k, 2.0);
            if k < parts {
                conduct(frame + k, frame + k + 1, 0.5);
            }
        }
    }
    let n = count * (parts + 2);
    positions.extend((0..n).map(|i| (i, i)));
    values.extend(std::iter::repeat_n(0.05, n));
    Assembly::new(n, &positions).matrix(&values)
}

#[test]
fn the_parallel_cholesky_gives_the_bits_of_the_sequential_one() {
    for a in [machines(400, 20), grid(80, 0.01)] {
        assert!(a.n >= 4096, "large enough to be done in parallel");
        let b: Vec<f64> = (0..a.n)
            .map(|i| f64::from(u32::try_from(i % 23).unwrap()) - 11.0)
            .collect();
        let mut parallel = Cholesky::new(&a).unwrap();
        let mut sequential = Cholesky::sequential(&a).unwrap();
        let (mut x, mut y) = (b.clone(), b.clone());
        parallel.solve(&mut x);
        sequential.solve(&mut y);
        assert_eq!(bits(&x), bits(&y));
        assert!(residual(&a, &x, &b) < 1e-8, "{}", residual(&a, &x, &b));
    }
}
