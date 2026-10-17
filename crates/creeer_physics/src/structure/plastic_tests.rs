//! Plasticity and fracture against plastic-hinge theory.

use super::section::STEEL_S235;
use super::shapes::cantilever;
use super::*;
use super::{NodeId, StructureBuilder, Support};

const DT: f64 = 1.0 / 60.0;

/// HEA 200 from the steel tables: W_pl,y = 429.5 cm³. The simple shape leaves out the root
/// fillets: 5 % less.
#[test]
fn plastic_modulus_of_an_i_beam_matches_the_tables() {
    let s = Section::i_beam(0.19, 0.2, 0.01, 0.0065);
    assert!(
        (s.plastic_modulus[0] / 429.5e-6 - 1.0).abs() < 0.06,
        "{}",
        s.plastic_modulus[0]
    );
}

/// A 2 m cantilever of 50 mm round steel bar (10 elements) with a tip load ramped up over
/// 3 s to `peak` (N), held for 1 s and, with `unload`, ramped back to zero over 1 s.
/// Returns the structure and the load at which the first member yielded.
fn load_cantilever(peak: f64, unload: bool) -> (Structure, Option<f64>) {
    // Plastic hinge theory: perfectly plastic steel (hardening: `hardening_tests.rs`).
    let material = Material {
        damping: 0.01,
        ..STEEL_S235.perfectly_plastic()
    };
    let mut s = cantilever(2.0, 10, Section::solid_round(0.05), material);
    s.gravity = DVec3::ZERO;
    let mut solver = ImplicitSolver::new(&s, Scheme::Newmark, DT, 1);
    let mut first_yield = None;
    let ticks = if unload { 300 } else { 240 };
    for tick in 0..ticks {
        let t = f64::from(tick) * DT;
        let load = if t < 3.0 {
            peak * t / 3.0
        } else if t < 4.0 || !unload {
            peak
        } else {
            peak * (5.0 - t).max(0.0)
        };
        s.load[10] = DVec3::NEG_Y * load;
        solver.step(&mut s);
        if first_yield.is_none() && solver.last_yielding.yielded > 0 {
            first_yield = Some(load);
        }
    }
    (s, first_yield)
}

/// The collapse load of a cantilever is P_c = M_p/L (M_p = σ_y·d³/6): below it the bar
/// stays elastic, at it a plastic hinge forms at the root.
#[test]
fn a_cantilever_collapses_at_its_plastic_load() {
    let m_p = STEEL_S235.yield_strength * (0.05 * 0.05 * 0.05) / 6.0;
    let collapse = m_p / 2.0;
    let (below, yielded) = load_cantilever(0.95 * collapse, false);
    assert_eq!(yielded, None, "yielded below the collapse load");
    assert!(below.elements.iter().all(|e| e.plastic_strain == 0.0));
    let (above, yielded) = load_cantilever(1.05 * collapse, false);
    let first = yielded.expect("yields above the collapse load");
    // The ramp reaches P_c 5 % before its peak; the root element's moment is the tip load
    // times the full length minus half an element.
    assert!(
        (first / (collapse / 0.95) - 1.0).abs() < 0.05,
        "first yield at {first} N, P_c {collapse} N"
    );
    let root = &above.elements[0];
    let worst = above
        .elements
        .iter()
        .map(|e| e.plastic_strain)
        .fold(0.0, f64::max);
    assert!(
        root.plastic_strain >= worst && worst > 0.0,
        "the hinge forms at the root"
    );
}

/// Loaded just past collapse and unloaded, the bar springs back elastically but keeps a
/// permanent bend.
#[test]
fn unloading_leaves_a_permanent_set() {
    let m_p = STEEL_S235.yield_strength * (0.05 * 0.05 * 0.05) / 6.0;
    let (s, _) = load_cantilever(1.02 * m_p / 2.0, true);
    let set = -s.displacement(10).y;
    assert!(set > 0.01, "permanent tip set {set} m");
}

/// A perfectly plastic bar pulled with more than its plastic capacity N_p = σ_y·A (a 1 t
/// mass at its end, pulled with 1.2·N_p) stretches until its plastic strain passes the
/// elongation at break (26 %), then breaks: at its weakest element (1 % less area, as real
/// bars are never perfectly uniform), and only there. No member passes more than its
/// capacity, so the chain carries 0.99·N_p, the weakest element flows and the others stay
/// elastic; the rest of the load accelerates the mass, and the break releases it. (Real
/// steel hardens to carry 1.2·N_p, `hardening_tests.rs`; past its ultimate strength it
/// would neck at its weakest point, which is not modelled: ADR-0005.)
#[test]
fn an_overloaded_bar_tears() {
    let section = Section::solid_round(0.02);
    let mut b = StructureBuilder::new();
    let nodes: Vec<NodeId> = (0..=4)
        .map(|i| b.node(DVec3::X * (0.25 * f64::from(i))))
        .collect();
    b.support(nodes[0], Support::Clamped);
    for k in 0..4 {
        let thinner = Section::solid_round(0.02 * 0.995);
        b.beam(
            nodes[k],
            nodes[k + 1],
            if k == 2 { thinner } else { section },
            STEEL_S235.perfectly_plastic(),
        );
    }
    b.mass(nodes[4], 1000.0, DMat3::from_diagonal(DVec3::splat(10.0)));
    let mut s = b.build();
    s.gravity = DVec3::ZERO;
    let capacity = STEEL_S235.yield_strength * section.area;
    s.load[4] = DVec3::X * 1.2 * capacity;
    let mut solver = ImplicitSolver::new(&s, Scheme::Newmark, DT, 1);
    let mut broke_at = None;
    for tick in 0..600 {
        solver.step(&mut s);
        if solver.last_yielding.broke > 0 {
            broke_at = Some(tick);
            break;
        }
    }
    assert!(broke_at.is_some(), "never broke");
    let broken = s.elements.iter().filter(|e| e.broken).count();
    assert_eq!(broken, 1, "exactly one element tears");
    assert!(s.elements[2].broken, "the thinnest element tears");
    let e = &s.elements[2];
    assert!(e.plastic_strain > STEEL_S235.elongation);
}
