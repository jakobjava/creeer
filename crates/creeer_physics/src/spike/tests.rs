use super::*;

const DT: f64 = 1.0 / 60.0;

#[test]
fn spike_scene_has_about_five_thousand_beams() {
    let world = TrussWorld::bridges(16);
    assert_eq!(world.trusses[0].beam_count(), 317);
    assert_eq!(world.beam_count(), 5072);
}

#[test]
fn supports_hold_along_their_fixed_axes_only() {
    let mut truss = Truss::box_bridge(DVec3::ZERO, BridgeSpec::REFERENCE);
    let pins: Vec<usize> = (0..truss.node_count())
        .filter(|&i| truss.inv_mass[i] == DVec3::ZERO)
        .collect();
    let rollers: Vec<usize> = (0..truss.node_count())
        .filter(|&i| truss.inv_mass[i].x > 0.0 && truss.inv_mass[i].y == 0.0)
        .collect();
    assert_eq!((pins.len(), rollers.len()), (2, 2));
    for _ in 0..120 {
        truss.step(DT, 20);
    }
    for i in pins {
        assert_eq!(truss.position[i], truss.rest[i], "pinned node {i} moved");
    }
    for i in rollers {
        let (p, r) = (truss.position[i], truss.rest[i]);
        assert_eq!((p.y, p.z), (r.y, r.z), "roller {i} left its rail");
        // Under load the bottom chord lengthens, so the roller slides outwards.
        assert!(p.x > r.x, "roller {i} did not slide: {} -> {}", r.x, p.x);
    }
}

#[test]
fn exact_fem_agrees_with_timoshenko_beam_theory() {
    // Validates the FEM reference itself. A discrete 24-bay truss is not exactly a
    // continuous beam, so a few percent of difference is expected (measured: 5.1 %).
    let spec = BridgeSpec::REFERENCE;
    let truss = Truss::box_bridge(DVec3::ZERO, spec);
    let beam_theory = analytic_midspan_sag(&spec, truss.total_mass(&spec));
    let fem = linear_static_sag(spec);
    let ratio = fem / beam_theory;
    assert!(
        (0.93..=1.0).contains(&ratio),
        "FEM/beam theory = {ratio:.4}"
    );
}

#[test]
fn xpbd_converges_to_the_exact_solution_with_enough_substeps() {
    let exact = linear_static_sag(BridgeSpec::REFERENCE);
    let ratio = measure_static_sag(100) / exact;
    // Measured 1.047 at 100 substeps (1.012 at 200).
    assert!(
        (1.0..1.08).contains(&ratio),
        "XPBD/FEM at 100 substeps = {ratio:.4}"
    );
}

/// Characterisation of the spike's key finding (ADR-0002): at the originally planned 8
/// substeps, XPBD makes steel far too soft. If a solver change fixes this, update the
/// ADR, then this test.
#[test]
fn xpbd_at_eight_substeps_is_much_too_soft_for_steel() {
    let exact = linear_static_sag(BridgeSpec::REFERENCE);
    let ratio = measure_static_sag(8) / exact;
    assert!(ratio > 5.0, "XPBD/FEM at 8 substeps = {ratio:.2}");
}

#[test]
fn parallel_and_sequential_stepping_are_bit_identical() {
    let mut a = TrussWorld::bridges(4);
    let mut b = a.clone();
    for _ in 0..30 {
        a.step_sequential(DT, 8);
        b.step_parallel(DT, 8);
    }
    assert_eq!(a.state_hash(), b.state_hash());
}

/// Cross-platform determinism: the exact same bits on Windows, Linux and macOS.
/// Generated on Windows x86-64; never regenerate it to make a failure go away.
#[test]
fn state_hash_is_identical_on_every_platform() {
    let mut world = TrussWorld::bridges(2);
    for _ in 0..120 {
        world.step_sequential(DT, 8);
    }
    let hash = world.state_hash();
    if std::env::var_os("CREEER_PRINT_GOLDEN").is_some() {
        println!("truss golden hash: {hash:#018x}");
    }
    assert_eq!(hash, GOLDEN_TRUSS_HASH, "got {hash:#018x}");
}

const GOLDEN_TRUSS_HASH: u64 = 0x8d6e_356f_4f62_e490;
