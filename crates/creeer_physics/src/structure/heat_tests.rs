//! Members at temperature against EN 1993-1-2 and elementary thermoelasticity.

use super::heat::BUILT_AT;
use super::*;
use creeer_materials::MaterialTable;

fn steel() -> creeer_materials::Material {
    MaterialTable::reference()
        .get("steel_s235")
        .unwrap()
        .clone()
}

/// A bar along +x of `elements` members, clamped at x = 0 and, if `held`, at its far end.
fn bar(length: f64, elements: usize, held: bool) -> Structure {
    let mut b = StructureBuilder::new();
    #[allow(clippy::cast_precision_loss)]
    let nodes: Vec<NodeId> = (0..=elements)
        .map(|i| b.node(DVec3::X * (length * i as f64 / elements as f64)))
        .collect();
    b.support(nodes[0], Support::Clamped);
    if held {
        b.support(nodes[elements], Support::Clamped);
    }
    let material = Material::from(steel().structural(BUILT_AT));
    for pair in nodes.windows(2) {
        b.beam(pair[0], pair[1], Section::round_tube(0.1, 0.005), material);
    }
    let mut s = b.build();
    s.gravity = DVec3::ZERO;
    s
}

fn heat_all(s: &mut Structure, t: f64) {
    let steel = steel();
    for k in 0..s.element_count() {
        s.set_temperature(k, t, &steel);
    }
}

/// A free bar heated from 20 °C to 520 °C lengthens by its EN 1993-1-2 thermal strain,
/// 1.2e-5·θ + 0.4e-8·θ² − 2.416e-4 = 7.08e-3 (7 mm a metre), and no force builds up.
#[test]
fn a_free_bar_lengthens_by_its_thermal_strain() {
    let (l, c) = (2.0, 520.0);
    let mut s = bar(l, 4, false);
    heat_all(&mut s, 273.15 + c);
    statics::nonlinear(&mut s, 50);
    let en = 1.2e-5 * c + 0.4e-8 * c * c - 2.416e-4;
    let stretch = s.displacement(4).x;
    assert!(
        (stretch / (l * en) - 1.0).abs() < 1e-6,
        "{stretch} vs {}",
        l * en
    );
    for e in &s.elements {
        let (x, r) = (&s.position, &s.orientation);
        let m = e.measures(
            x[e.a],
            creeer_math::glam::DMat3::from_quat(r[e.a]),
            x[e.b],
            creeer_math::glam::DMat3::from_quat(r[e.b]),
        );
        // The static solve converges to ~1e-6 of the force the strain would make if held.
        assert!(m[0].abs() < 1e-6 * l * en, "no strain left: {}", m[0]);
    }
}

/// The same bar held at both ends and warmed by 40 K pushes on its supports with
/// N = E·A·ε_th (elastic: 105 MPa, under its yield strength), and does not move.
#[test]
fn a_held_bar_pushes_with_e_a_alpha_delta_t() {
    let mut s = bar(2.0, 4, true);
    let t = BUILT_AT + 40.0;
    heat_all(&mut s, t);
    statics::nonlinear(&mut s, 50);
    let steel = steel();
    let strain = steel.expansion.integral(BUILT_AT, t);
    let e = &s.elements[1];
    let (x, r) = (&s.position, &s.orientation);
    let m = e.measures(
        x[e.a],
        creeer_math::glam::DMat3::from_quat(r[e.a]),
        x[e.b],
        creeer_math::glam::DMat3::from_quat(r[e.b]),
    );
    let force = e.stiffness.axial * m[0];
    let want = -steel.youngs_modulus.at(t) * e.section.area * strain;
    assert!((force / want - 1.0).abs() < 1e-6, "{force} vs {want} N");
    assert!((want / e.section.area).abs() < 0.5 * steel.yield_strength.at(t));
}

/// At 600 °C steel keeps 31 % of its stiffness (EN 1993-1-2 Table 3.1, k_E): a cantilever's
/// tip deflects 1/0.31 as far under the same load.
#[test]
fn hot_steel_is_softer_by_en_1993_1_2() {
    let mut cold = bar(3.0, 6, false);
    cold.load[6] = DVec3::Y * -1000.0;
    let mut hot = cold.clone();
    heat_all(&mut hot, 273.15 + 600.0);
    let (u_cold, _) = statics::linear(&cold).unwrap();
    let (u_hot, _) = statics::linear(&hot).unwrap();
    let ratio = u_hot[6].y / u_cold[6].y;
    assert!((ratio * 0.31 - 1.0).abs() < 1e-3, "{ratio}");
}

/// Cooled back to 20 °C, a member is as it was: as long, as stiff, as strong.
#[test]
fn cooling_undoes_heating() {
    let mut s = bar(2.0, 2, false);
    let before = s.elements[0];
    let steel = steel();
    let ratio = s.set_temperature(0, 1000.0, &steel);
    assert!(
        ratio < 0.2,
        "steel at 727 °C has lost most of its stiffness: {ratio}"
    );
    s.set_temperature(0, BUILT_AT, &steel);
    let after = s.elements[0];
    assert!(after.thermal.abs() < 1e-18);
    assert_eq!(after.stiffness, before.stiffness);
    assert_eq!(after.material, before.material);
}

/// A hot member split in two (buckling) stays as long: each half keeps half its elongation.
#[test]
fn a_hot_member_split_in_two_stays_as_long() {
    let mut s = bar(2.0, 2, false);
    let steel = steel();
    s.set_temperature(1, 873.15, &steel);
    let (whole, stiffness) = (s.elements[1].thermal, s.elements[1].material);
    s.split_member(1);
    assert_eq!(s.element_count(), 3);
    assert!((s.elements[1].thermal + s.elements[2].thermal - whole).abs() < 1e-18);
    assert_eq!(
        (s.elements[1].material, s.elements[2].material),
        (stiffness, stiffness)
    );
}
