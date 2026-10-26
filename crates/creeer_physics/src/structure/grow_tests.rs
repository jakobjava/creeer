//! Growing a structure as it is built (`grow.rs`): what it holds is kept, and a new joint
//! holds its nodes where they are.

use super::section::STEEL_S235;
use super::shapes::cantilever;
use super::*;
use super::{JointLaw, Support};
use crate::so3;

const NAILED: JointLaw = JointLaw {
    axial: 5.0e6,
    bearing: 20.0e3,
    withdrawal: 600.0,
    lateral: 7.0e5,
    lateral_yield: 1000.0,
    lateral_peak: 1300.0,
    slip_at_peak: 0.008,
    slip_at_failure: 0.015,
    twist: 50.0,
    twist_yield: 2.0,
    interaction: 1.0,
    damping: 0.002,
};

/// A 2 m cantilever of 50 mm steel bar, sagged under its weight (large-deformation statics).
fn sagged() -> Structure {
    let mut s = cantilever(2.0, 4, Section::solid_round(0.05), STEEL_S235);
    statics::nonlinear(&mut s, 50);
    s
}

fn momentum(s: &Structure) -> DVec3 {
    (s.masses().iter().zip(&s.velocity))
        .map(|(m, v)| *v * *m)
        .sum()
}

/// A node put into a bent, moving member keeps the structure's mass, momentum and energy,
/// and lies on the member's curve.
#[test]
fn a_node_inserted_into_a_member_keeps_what_the_structure_holds() {
    let mut s = sagged();
    for (i, v) in s.velocity.iter_mut().enumerate() {
        *v = DVec3::new(0.1, 0.02 * i as f64, -0.05);
    }
    let (mass, p, elastic) = (s.total_mass(), momentum(&s), s.elastic_energy());
    let n = s.insert_node(2, 0.3);
    assert!((s.total_mass() / mass - 1.0).abs() < 1e-14);
    assert!((momentum(&s) - p).length() < 1e-14 * p.length());
    // The member's two pieces bend as it did: a cubic's pieces store about what it stored.
    assert!(
        (s.elastic_energy() / elastic - 1.0).abs() < 0.01,
        "{} against {elastic}",
        s.elastic_energy()
    );
    let (a, b) = (
        s.elements[2].a,
        s.elements.last().map(|e| e.b).expect("a piece"),
    );
    let along = (s.position[n] - s.position[a]).length() / (s.position[b] - s.position[a]).length();
    assert!((along - 0.3).abs() < 0.01, "{along}");
}

/// Joined where they lie, even with their nodes turned apart, two structures hold still: the
/// new joint measures nothing, so nothing moves.
#[test]
fn a_joint_between_turned_nodes_holds_them_where_they_are() {
    let mut s = sagged();
    let tip = s.node_count() - 1;
    let mut other = cantilever(1.0, 2, Section::solid_round(0.04), STEEL_S235);
    // The second piece turned and moved beside the first's tip, free.
    let turn = so3::exp(DVec3::new(0.3, -0.7, 0.4));
    for i in 0..other.node_count() {
        other.position[i] = s.position[tip] + DVec3::Z * 0.045 + turn * other.position[i];
        other.orientation[i] = turn;
    }
    other.set_support(0, Support::Free);
    let at = s.append(&other);
    let before = s.elastic_energy();
    let joint = s.join(tip, at, &NAILED);
    assert!(s.elements[joint].is_joint());
    assert!(
        (s.elastic_energy() - before).abs() < 1e-9,
        "the joint stores {}",
        s.elastic_energy() - before
    );
}

/// A part fastened onto a loaded structure: the structure's sag is kept, not sprung back.
#[test]
fn building_onto_a_loaded_structure_keeps_its_load() {
    let mut s = sagged();
    let tip = s.node_count() - 1;
    let sag = s.position[tip].y;
    let mut other = cantilever(0.5, 1, Section::solid_round(0.03), STEEL_S235);
    for p in &mut other.position {
        *p += s.position[tip] + DVec3::Z * 0.04;
    }
    other.set_support(0, Support::Free);
    let at = s.append(&other);
    s.join(tip, at, &NAILED);
    statics::nonlinear(&mut s, 50);
    // It sags a little more under the new part's weight (2.8 kg at the tip), never less.
    assert!(
        s.position[tip].y < sag,
        "{} against {sag}",
        s.position[tip].y
    );
    assert!(
        s.position[tip].y > sag - 0.01,
        "{} against {sag}",
        s.position[tip].y
    );
}

/// A bar fixed to the world at one end by a joint across it (G2.9: nailed to a standing tree)
/// and pinned at the other: the joint carries its share of the weight, slipping by that over
/// its slip modulus, and the fixed node does not move.
#[test]
fn a_bar_fixed_to_the_world_by_a_joint_carries_its_end() {
    let mut s = cantilever(2.0, 4, Section::solid_round(0.05), STEEL_S235);
    let tip = s.node_count() - 1;
    s.set_support(0, Support::Free);
    s.set_support(tip, Support::Pinned);
    // The fixed node 0.15 m beside the bar's end, across the weight.
    let at = s.position[0] - DVec3::Z * 0.15;
    let joint = s.fix_to(0, at, &NAILED);
    let fixed = s.elements[joint].a;
    assert!(s.held_fast(fixed));
    statics::nonlinear(&mut s, 50);
    // Half the bar's weight at its end (the fixed node's mass is never moved).
    let half = 0.5 * (s.total_mass() - 1.0) * STANDARD_GRAVITY.length();
    let slip = -s.position[0].y;
    let expected = half / NAILED.lateral;
    assert!(
        (slip / expected - 1.0).abs() < 0.3,
        "slipped {slip} m, {expected} m expected"
    );
    assert!((s.position[fixed] - at).length() < 1e-12, "the world held");
    assert_eq!(s.elements_broken(), 0);
}
