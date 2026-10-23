//! Lateral-torsional buckling (G2.5, ADR-0033): a deep, narrow beam bent about its strong
//! axis kicks out sideways and twists at its critical moment. With fork supports and a uniform
//! moment, M_cr = (π/L)·√(E·I_weak·G·J) for a section without warping stiffness (a solid
//! rectangle: Timoshenko & Gere, *Theory of Elastic Stability*, §6.2). The co-rotational
//! members carry it in their exact rotations, without a warping degree of freedom.

use super::*;
use super::{StructureBuilder, Support};

const DT: f64 = 1.0 / 60.0;

/// A 6 m spruce beam, 300 mm deep (along y) and 50 mm wide (along z), in 16 members, on fork
/// supports (ends held across and from twisting, free to bend), bent by end moments ramped to
/// `moment` (N·m) over 10 s and held 2 s, a hair of sideways load at mid-span to start it.
/// Its middle's sideways travel at the end (m).
fn bent(moment: f64) -> f64 {
    let material = Material {
        youngs_modulus: 11.0e9,
        shear_modulus: 0.69e9,
        density: 420.0,
        yield_strength: 1.0e12,
        ultimate_strength: 1.0e12,
        damping: 0.02,
        ..section::STEEL_S235
    };
    let section = Section::rectangle(0.3, 0.05);
    let mut b = StructureBuilder::new();
    let nodes: Vec<NodeId> = (0..=16)
        .map(|k| b.node(DVec3::X * (6.0 * f64::from(k) / 16.0)))
        .collect();
    for w in nodes.windows(2) {
        b.beam_oriented(w[0], w[1], section, material, DVec3::Y);
    }
    // Forks: held across and up, from twisting about the beam's axis; one end along it too.
    let fork = |along: f64| Support::Custom {
        translation: DVec3::new(along, 0.0, 0.0),
        rotation: DVec3::new(0.0, 1.0, 1.0),
    };
    b.support(nodes[0], fork(0.0));
    b.support(nodes[16], fork(1.0));
    let mut s = b.build();
    s.gravity = DVec3::ZERO;
    s.splits = false;
    let mut solver = ImplicitSolver::new(&s, Scheme::Newmark, DT, 1);
    for tick in 0..720 {
        let m = moment * (f64::from(tick) / 600.0).min(1.0);
        s.torque[nodes[0]] = DVec3::Z * -m;
        s.torque[nodes[16]] = DVec3::Z * m;
        s.load[nodes[8]] = DVec3::Z * 1.0;
        solver.step(&mut s);
    }
    s.displacement(nodes[8]).z
}

#[test]
/// (Measured: 0.6 mm at 0.9·M_cr, 1.5 at 0.97, 25 at 1.03, 458 at 1.1: within 3 %.)
fn a_deep_narrow_beam_buckles_sideways_at_its_critical_moment() {
    let section = Section::rectangle(0.3, 0.05);
    let (e, g, l) = (11.0e9, 0.69e9, 6.0);
    let weak = section.i_y.min(section.i_z);
    let critical = std::f64::consts::PI / l * (e * weak * g * section.torsion).sqrt();
    let below = bent(0.9 * critical).abs();
    let above = bent(1.1 * critical).abs();
    assert!(
        below < 0.02,
        "at 0.9 M_cr ({critical:.0} N·m) it moved {below} m sideways"
    );
    assert!(above > 0.1, "at 1.1 M_cr it moved only {above} m sideways");
}
