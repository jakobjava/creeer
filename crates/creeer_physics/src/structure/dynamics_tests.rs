//! Dynamic behaviour of structures: damping, impacts, free rotation, determinism.

use super::section::STEEL_S235;
use super::shapes::{Base, cantilever, space_frame};
use super::*;
use crate::spike::oscillation;

const DT: f64 = 1.0 / 60.0;

fn tube() -> Section {
    Section::round_tube(0.1, 0.005)
}

/// Steel's stiffness-proportional damping c gives a mode of angular frequency ω the damping
/// ratio ζ = c·ω/2: 0.85 % for the cantilever's 2.7 Hz first mode. Newmark adds none of its
/// own, so the measured ζ must be the physical one.
#[test]
fn material_damping_matches_theory() {
    let mut s = cantilever(6.0, 20, tube(), STEEL_S235);
    s.gravity = DVec3::ZERO;
    s.load[20] = DVec3::NEG_Y * 500.0;
    statics::nonlinear(&mut s, 50);
    s.load[20] = DVec3::ZERO;
    let mut solver = ImplicitSolver::new(&s, Scheme::Newmark, DT, 1);
    let samples: Vec<f64> = (0..600)
        .map(|_| {
            solver.step(&mut s);
            s.displacement(20).y
        })
        .collect();
    let o = oscillation(&samples, DT, 0.0).expect("oscillates");
    let want = STEEL_S235.damping * 2.0 * std::f64::consts::PI * o.frequency / 2.0;
    assert!(
        (o.damping_ratio / want - 1.0).abs() < 0.1,
        "ζ {} vs {want}",
        o.damping_ratio
    );
}

/// A violent impact on a welded frame: one top corner kicked at 30 m/s. Energy may only go
/// down (damping), never up, and nothing may explode.
#[test]
fn an_impact_never_adds_energy() {
    let mut s = space_frame([3, 2, 3], 2.0, tube(), STEEL_S235, Base::Corners);
    let corner = s.node_count() - 1;
    s.velocity[corner] = DVec3::new(-20.0, -20.0, 10.0);
    let start = s.energy();
    let mut solver = ImplicitSolver::new(&s, Scheme::Newmark, DT, 1);
    for tick in 0..300 {
        solver.step(&mut s);
        let energy = s.energy();
        assert!(
            energy.is_finite() && energy <= start * 1.0001,
            "tick {tick}: {energy} > {start}"
        );
    }
    assert!(
        s.energy() < 0.5 * start,
        "the frame should have damped most of it"
    );
}

/// A free frame tumbling at about 2 rad/s for 10 s without damping: its energy and shape
/// stay intact (Newmark is a variational integrator: bounded energy error, no drift), and
/// it keeps turning at the right rate.
#[test]
fn a_tumbling_frame_keeps_its_energy_and_shape() {
    let material = Material {
        damping: 0.0,
        ..STEEL_S235
    };
    let mut s = space_frame([2, 2, 2], 1.5, tube(), material, Base::Free);
    s.gravity = DVec3::ZERO;
    let omega = DVec3::new(0.3, 2.0, -0.5);
    let centre = s.position.iter().sum::<DVec3>() / s.node_count() as f64;
    for i in 0..s.node_count() {
        s.velocity[i] = omega.cross(s.position[i] - centre);
        s.angular_velocity[i] = omega;
    }
    let start = s.energy();
    let mut solver = ImplicitSolver::new(&s, Scheme::Newmark, DT, 1);
    for _ in 0..600 {
        solver.step(&mut s);
        let drift = (s.energy() / start - 1.0).abs();
        assert!(drift < 0.01, "energy drift {drift}");
        assert!(
            s.max_axial_strain() < 1e-4,
            "strain {}",
            s.max_axial_strain()
        );
    }
}

/// Cross-platform determinism of the full structure step (beam kinematics, rotations,
/// L-BFGS, line search, sparse solve). Generated on Windows x86-64; never regenerate it to
/// make a failure go away.
#[test]
fn state_hash_is_identical_on_every_platform() {
    let mut s = space_frame([2, 1, 2], 2.0, tube(), STEEL_S235, Base::Corners);
    let corner = s.node_count() - 1;
    s.velocity[corner] = DVec3::new(3.0, -5.0, 1.0);
    let mut solver = ImplicitSolver::new(&s, Scheme::Newmark, DT, 1);
    for _ in 0..120 {
        solver.step(&mut s);
    }
    let hash = s.state_hash();
    if std::env::var_os("CREEER_PRINT_GOLDEN").is_some() {
        println!("structure golden hash: {hash:#018x}");
    }
    assert_eq!(hash, GOLDEN_STRUCTURE_HASH, "got {hash:#018x}");
}

// Regenerated once, for a deliberate change of the model: member damping moved in series
// with the plastic slider (ADR-0005). The old value came back exactly with the old damper.
const GOLDEN_STRUCTURE_HASH: u64 = 0x3c29_4c75_dfec_43b5;
