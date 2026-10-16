//! F2 solver comparison (ADR-0004): the references are validated against each other, and
//! each candidate's measured behaviour is pinned down. Measured values are in the comments.

use super::*;

const DT: f64 = 1.0 / 60.0;
/// Four seconds: about seven periods of the bridge's first mode.
const TICKS: usize = 240;

fn reference() -> Truss {
    Truss::box_bridge(DVec3::ZERO, BridgeSpec::REFERENCE)
}

fn response(mut step: impl FnMut() -> f64) -> Vec<f64> {
    (0..TICKS).map(|_| step()).collect()
}

fn implicit_response(scheme: Scheme, substeps: u32) -> Oscillation {
    let mut sim = ImplicitTruss::new(reference(), scheme, DT, substeps);
    let samples = response(|| {
        sim.step();
        sim.truss.midspan_sag()
    });
    let exact = linear_static_sag(BridgeSpec::REFERENCE);
    oscillation(&samples, DT, exact).expect("bridge oscillates")
}

#[test]
fn sparse_static_solve_matches_the_dense_fem_reference() {
    let truss = reference();
    let u = sparse_static_displacement(&truss);
    let dense = linear_static_displacement(BridgeSpec::REFERENCE);
    let scale = dense.iter().map(|d| d.length()).fold(0.0, f64::max);
    for (sparse, dense) in u.iter().zip(&dense) {
        assert!((*sparse - *dense).length() < 1e-10 * scale);
    }
}

/// Two independent references for the first vertical frequency: Rayleigh's method from the
/// exact static shape (1.7737 Hz) and a brute-force explicit simulation (1.7715 Hz).
#[test]
fn rayleigh_and_explicit_references_agree() {
    let spec = BridgeSpec::REFERENCE;
    let samples = explicit_reference_response(spec, TICKS);
    let measured = oscillation(&samples, DT, linear_static_sag(spec)).expect("oscillates");
    let ratio = measured.frequency / rayleigh_frequency(spec);
    // Rayleigh is an upper bound for the fundamental frequency.
    assert!(
        (0.995..=1.0).contains(&ratio),
        "explicit/Rayleigh = {ratio:.4}"
    );
    // The explicit reference has (almost) no damping.
    assert!(measured.damping_ratio.abs() < 1e-3, "{measured:?}");
}

/// The chosen scheme at one step per tick: frequency −1.1 %, damping ratio 0.14 %.
#[test]
fn bdf2_reproduces_the_first_frequency_with_little_damping() {
    // The explicit reference's measured frequency (see the test above).
    let reference = rayleigh_frequency(BridgeSpec::REFERENCE) * 0.9988;
    for (substeps, max_error) in [(1, 0.02), (2, 0.005)] {
        let o = implicit_response(Scheme::Bdf2, substeps);
        let error = (o.frequency / reference - 1.0).abs();
        assert!(error < max_error, "x{substeps}: {o:?}");
        assert!(o.damping_ratio < 0.003, "x{substeps}: {o:?}");
    }
}

/// Backward Euler gets the frequency right too, but damps it away: ζ = 9 % at one step per
/// tick, against about 0.5–2 % for real steel structures.
#[test]
fn backward_euler_damps_far_too_much() {
    let o = implicit_response(Scheme::BackwardEuler, 1);
    assert!(o.damping_ratio > 0.05, "{o:?}");
}

/// Implicit schemes settle on the exact equilibrium (measured 1.0007 × linear FEM; the
/// remaining 0.07 % is geometric nonlinearity, which the linear reference ignores).
#[test]
fn implicit_static_equilibrium_is_exact() {
    let mut sim = ImplicitTruss::new(reference(), Scheme::BackwardEuler, DT, 1);
    for _ in 0..30 * 60 {
        sim.step();
    }
    let ratio = sim.truss.midspan_sag() / linear_static_sag(BridgeSpec::REFERENCE);
    assert!((ratio - 1.0).abs() < 0.002, "implicit/FEM = {ratio:.5}");
}

/// VBD is correct in the limit: with 5,000 iterations per step it reproduces backward Euler.
#[test]
fn vbd_converges_to_backward_euler_with_enough_iterations() {
    let mut vbd = VbdTruss::new(reference(), DT, 1, 5000);
    let mut implicit = ImplicitTruss::new(reference(), Scheme::BackwardEuler, DT, 1);
    implicit.max_iterations = 20;
    implicit.tolerance = 1e-12;
    for _ in 0..12 {
        vbd.step();
        implicit.step();
    }
    let ratio = vbd.truss.midspan_sag() / implicit.truss.midspan_sag();
    assert!((ratio - 1.0).abs() < 2e-3, "VBD/implicit = {ratio:.5}");
}

/// ...but at an affordable iteration count it is far off for steel: 200 iterations per step
/// give a first frequency 22 % too low (50 iterations: 51 % too low).
#[test]
fn vbd_needs_far_too_many_iterations_for_steel() {
    let mut sim = VbdTruss::new(reference(), DT, 1, 200);
    let samples = response(|| {
        sim.step();
        sim.truss.midspan_sag()
    });
    let exact = linear_static_sag(BridgeSpec::REFERENCE);
    let o = oscillation(&samples, DT, exact).expect("oscillates");
    let ratio = o.frequency / rayleigh_frequency(BridgeSpec::REFERENCE);
    assert!(ratio < 0.9, "VBD frequency ratio {ratio:.3}");
}

/// A spinning free body: the rest-frame matrix is wrong once the body has turned (member
/// strain 1.1, i.e. torn apart), the body-frame one tracks it (2.4e-5, the real centrifugal
/// stretch: ω²r²ρ/E ≈ 1e-5).
#[test]
fn a_spinning_body_needs_the_body_frame() {
    let max_strain = |linearisation| {
        let mut truss = Truss::free_block([3, 3, 3]);
        truss.spin(DVec3::new(0.4, 3.0, 0.7));
        let mut sim = ImplicitTruss::new(truss, Scheme::Bdf2, DT, 1);
        sim.linearisation = linearisation;
        (0..600).fold(0.0, |worst: f64, _| {
            sim.step();
            worst.max(sim.truss.max_strain())
        })
    };
    assert!(max_strain(Linearisation::RestFrame) > 0.01);
    assert!(max_strain(Linearisation::BodyFrame) < 1e-4);
}

/// A violent impact (midspan kicked at 30 m/s): a single linearised step explodes; the
/// line-searched Newton iterations never add energy.
#[test]
fn an_impact_never_adds_energy() {
    for scheme in [Scheme::BackwardEuler, Scheme::Bdf2] {
        let mut truss = reference();
        for i in truss.midspan_nodes {
            truss.velocity[i] = DVec3::new(0.0, -30.0, 5.0);
        }
        let start = truss.energy();
        let mut sim = ImplicitTruss::new(truss, scheme, DT, 1);
        for tick in 0..300 {
            sim.step();
            let energy = sim.truss.energy();
            assert!(
                energy.is_finite() && energy <= start,
                "{scheme:?} tick {tick}: {energy}"
            );
        }
    }
}

/// Cross-platform determinism of the whole implicit step (sparse solve, rotation tracking,
/// line search). Generated on Windows x86-64; never regenerate it to make a failure go away.
#[test]
fn implicit_state_hash_is_identical_on_every_platform() {
    let mut truss = Truss::free_block([2, 2, 3]);
    truss.spin(DVec3::new(0.5, 2.0, -1.0));
    let mut sim = ImplicitTruss::new(truss, Scheme::Bdf2, DT, 1);
    for _ in 0..120 {
        sim.step();
    }
    let hash = sim.truss.state_hash();
    if std::env::var_os("CREEER_PRINT_GOLDEN").is_some() {
        println!("implicit golden hash: {hash:#018x}");
    }
    assert_eq!(hash, GOLDEN_IMPLICIT_HASH, "got {hash:#018x}");
}

const GOLDEN_IMPLICIT_HASH: u64 = 0xf16c_0b29_cec4_9422;
