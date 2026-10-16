use super::*;

/// Small deterministic generator (xorshift64*), so tests need no `rand` dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        #[allow(clippy::cast_precision_loss)]
        let bits = (self.0.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 11) as f64;
        bits / (1u64 << 53) as f64 * 2.0 - 1.0
    }

    fn mat(&mut self) -> DMat3 {
        DMat3::from_cols_array(&std::array::from_fn(|_| self.next()))
    }

    fn vec(&mut self) -> DVec3 {
        DVec3::new(self.next(), self.next(), self.next())
    }
}

/// An `nx × ny` grid of nodes with springs to the right, up and diagonally.
fn grid(nx: usize, ny: usize) -> (Vec<DVec3>, Vec<(usize, usize)>) {
    let id = |x: usize, y: usize| y * nx + x;
    #[allow(clippy::cast_precision_loss)]
    let positions = (0..ny)
        .flat_map(|y| (0..nx).map(move |x| DVec3::new(x as f64, y as f64, 0.0)))
        .collect();
    let mut edges = Vec::new();
    for y in 0..ny {
        for x in 0..nx {
            if x + 1 < nx {
                edges.push((id(x, y), id(x + 1, y)));
            }
            if y + 1 < ny {
                edges.push((id(x, y), id(x, y + 1)));
            }
            if x + 1 < nx && y + 1 < ny {
                edges.push((id(x + 1, y + 1), id(x, y)));
            }
        }
    }
    (positions, edges)
}

/// A random symmetric positive-definite matrix with the given pattern: random off-diagonal
/// blocks and diagonal blocks made dominant enough.
fn random_spd(rng: &mut Rng, n: usize, edges: &[(usize, usize)]) -> (Vec<DMat3>, Vec<DMat3>) {
    let off: Vec<DMat3> = edges.iter().map(|_| rng.mat()).collect();
    let mut diag: Vec<DMat3> = (0..n)
        .map(|_| {
            let m = rng.mat();
            m * m.transpose() + DMat3::IDENTITY
        })
        .collect();
    for (&(a, b), block) in edges.iter().zip(&off) {
        // Gershgorin: |row sum| of every off-diagonal block, added to both diagonals.
        let bound: f64 = block.to_cols_array().iter().map(|v| v.abs()).sum();
        diag[a] += DMat3::IDENTITY * bound;
        diag[b] += DMat3::IDENTITY * bound;
    }
    (diag, off)
}

fn multiply(diag: &[DMat3], off: &[DMat3], edges: &[(usize, usize)], x: &[DVec3]) -> Vec<DVec3> {
    let mut y: Vec<DVec3> = diag.iter().zip(x).map(|(d, v)| *d * *v).collect();
    for (&(a, b), block) in edges.iter().zip(off) {
        y[a] += *block * x[b];
        y[b] += block.transpose() * x[a];
    }
    y
}

fn max_residual(
    diag: &[DMat3],
    off: &[DMat3],
    edges: &[(usize, usize)],
    x: &[DVec3],
    b: &[DVec3],
) -> f64 {
    multiply(diag, off, edges, x)
        .iter()
        .zip(b)
        .map(|(ax, b)| (*ax - *b).abs().max_element())
        .fold(0.0, f64::max)
}

#[test]
fn solves_random_systems_with_any_ordering() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let (positions, edges) = grid(9, 7);
    let n = positions.len();
    let (diag, off) = random_spd(&mut rng, n, &edges);
    let b: Vec<DVec3> = (0..n).map(|_| rng.vec()).collect();
    let reversed: Vec<usize> = (0..n).rev().collect();
    for sym in [
        SymbolicCholesky::with_ordering(&edges, (0..n).collect()),
        SymbolicCholesky::with_ordering(&edges, reversed),
        SymbolicCholesky::new(&positions, &edges),
    ] {
        let mut chol = Cholesky::new(sym);
        chol.factor(&diag, &off).expect("matrix is SPD");
        let mut x = b.clone();
        chol.solve(&mut x);
        let residual = max_residual(&diag, &off, &edges, &x, &b);
        assert!(residual < 1e-12, "residual {residual:e}");
    }
}

#[test]
fn duplicate_edges_are_summed() {
    let mut rng = Rng(7);
    let (positions, edges) = grid(4, 4);
    let n = positions.len();
    let (diag, off) = random_spd(&mut rng, n, &edges);
    // Split every off-diagonal block into two halves on two copies of the edge.
    let doubled_edges: Vec<_> = edges.iter().chain(&edges).copied().collect();
    let halves: Vec<DMat3> = off.iter().chain(&off).map(|m| *m * 0.5).collect();
    let b: Vec<DVec3> = (0..n).map(|_| rng.vec()).collect();
    let mut chol = Cholesky::new(SymbolicCholesky::new(&positions, &doubled_edges));
    chol.factor(&diag, &halves).expect("matrix is SPD");
    let mut x = b.clone();
    chol.solve(&mut x);
    assert!(max_residual(&diag, &off, &edges, &x, &b) < 1e-12);
}

#[test]
fn reports_matrices_that_are_not_positive_definite() {
    let (positions, edges) = grid(3, 3);
    let mut diag = vec![DMat3::IDENTITY * 4.0; positions.len()];
    let off = vec![DMat3::IDENTITY * -0.5; edges.len()];
    diag[4] = DMat3::from_diagonal(DVec3::new(1.0, -1.0, 1.0));
    let mut chol = Cholesky::new(SymbolicCholesky::new(&positions, &edges));
    assert_eq!(
        chol.factor(&diag, &off),
        Err(NotPositiveDefinite { node: 4 })
    );
}

#[test]
fn a_chain_produces_little_fill() {
    let positions: Vec<DVec3> = (0..50).map(|i| DVec3::X * f64::from(i)).collect();
    let edges: Vec<_> = (0..49).map(|i| (i, i + 1)).collect();
    let natural = SymbolicCholesky::with_ordering(&edges, (0..50).collect());
    assert_eq!(natural.factor_blocks(), 49);
    let dissected = SymbolicCholesky::new(&positions, &edges);
    // Nested dissection adds a little fill around its separators (measured 83 for 49 links).
    assert!(
        dissected.factor_blocks() < 2 * 49,
        "{}",
        dissected.factor_blocks()
    );
}

/// Nested dissection must beat the natural (row-by-row) order clearly on a 2D mesh, where
/// natural order fills the whole band. Measured on 80×80: 2.95× less fill, 4.8× less work
/// (the gap grows with size: 6.8× less work on 120×120).
#[test]
fn nested_dissection_reduces_fill_and_work_on_a_mesh() {
    let (positions, edges) = grid(80, 80);
    let n = positions.len();
    let natural = SymbolicCholesky::with_ordering(&edges, (0..n).collect());
    let dissected = SymbolicCholesky::new(&positions, &edges);
    let fill_ratio = natural.factor_blocks() as f64 / dissected.factor_blocks() as f64;
    let work_ratio = natural.factor_work() as f64 / dissected.factor_work() as f64;
    assert!(fill_ratio > 2.5, "fill natural/ND = {fill_ratio:.2}");
    assert!(work_ratio > 4.0, "work natural/ND = {work_ratio:.2}");
}

#[test]
fn ordering_is_a_permutation() {
    let (positions, edges) = grid(23, 11);
    let mut perm = nested_dissection(&positions, &edges);
    perm.sort_unstable();
    assert_eq!(perm, (0..positions.len()).collect::<Vec<_>>());
}

/// Cross-platform determinism: the solver must produce the same bits everywhere.
/// Generated on Windows x86-64; never regenerate it to make a failure go away.
#[test]
fn solution_bits_are_identical_on_every_platform() {
    let mut rng = Rng(42);
    let (positions, edges) = grid(12, 10);
    let n = positions.len();
    let (diag, off) = random_spd(&mut rng, n, &edges);
    let mut x: Vec<DVec3> = (0..n).map(|_| rng.vec()).collect();
    let mut chol = Cholesky::new(SymbolicCholesky::new(&positions, &edges));
    chol.factor(&diag, &off).expect("matrix is SPD");
    chol.solve(&mut x);
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in x
        .iter()
        .flat_map(|v| v.to_array())
        .flat_map(|c| c.to_bits().to_le_bytes())
    {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    if std::env::var_os("CREEER_PRINT_GOLDEN").is_some() {
        println!("sparse golden hash: {hash:#018x}");
    }
    assert_eq!(hash, GOLDEN_SOLVE_HASH, "got {hash:#018x}");
}

const GOLDEN_SOLVE_HASH: u64 = 0x3a0f_cb9a_6755_d87e;

/// The parallel factorisation and solve must give exactly the bits of the sequential ones,
/// whatever the number of threads: same tree of cuts, processed with and without
/// parallelism (a single leaf covering everything).
#[test]
fn parallel_and_sequential_results_are_bit_identical() {
    let mut rng = Rng(99);
    let (positions, edges) = grid(40, 40);
    let n = positions.len();
    let (diag, off) = random_spd(&mut rng, n, &edges);
    let b: Vec<DVec3> = (0..n).map(|_| rng.vec()).collect();
    let (perm, tree) = nested_dissection_tree(&positions, &edges);
    assert!(
        matches!(tree, Dissection::Cut(..)),
        "the mesh is large enough to be cut"
    );
    let mut results = Vec::new();
    for tree in [tree, Dissection::Leaf(n)] {
        let mut chol = Cholesky::new(SymbolicCholesky::with_dissection(
            &edges,
            perm.clone(),
            tree,
        ));
        chol.factor(&diag, &off).expect("matrix is SPD");
        let mut x = b.clone();
        chol.solve(&mut x);
        results.push(x);
    }
    let bits = |x: &[DVec3]| {
        x.iter()
            .flat_map(|v| v.to_array())
            .map(f64::to_bits)
            .collect::<Vec<_>>()
    };
    assert_eq!(bits(&results[0]), bits(&results[1]));
}
