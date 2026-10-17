//! Heat in structures against EN 1993-1-2, and its feedback into the structure.

use super::heat::MemberHeat;
use creeer_materials::MaterialTable;
use creeer_math::glam::DVec3;
use creeer_physics::structure::{BUILT_AT, Section, Structure, StructureBuilder, Support, statics};
use creeer_thermal::STEFAN_BOLTZMANN;
use creeer_thermal::environment::iso_834;

fn steel() -> creeer_materials::Material {
    MaterialTable::reference()
        .get("steel_s235")
        .unwrap()
        .clone()
}

/// A horizontal steel tube cantilever of `elements` members, 3 m long.
fn cantilever(elements: usize, section: Section) -> Structure {
    let mut b = StructureBuilder::new();
    #[allow(clippy::cast_precision_loss)]
    let nodes: Vec<_> = (0..=elements)
        .map(|i| b.node(DVec3::X * (3.0 * i as f64 / elements as f64)))
        .collect();
    b.support(nodes[0], Support::Clamped);
    let material = creeer_physics::structure::Material::from(steel().structural(BUILT_AT));
    for w in nodes.windows(2) {
        b.beam(w[0], w[1], section, material);
    }
    let mut s = b.build();
    s.gravity = DVec3::ZERO;
    s
}

/// An unprotected steel tube (A_m/V = 105 m⁻¹) engulfed in the ISO 834 fire heats as
/// EN 1993-1-2 4.2.5.1's incremental method has it, step for step: within 2 K over an hour.
#[test]
fn a_steel_member_heats_in_a_standard_fire_as_en_1993_1_2_says() {
    let section = Section::round_tube(0.2, 0.01);
    let mut s = cantilever(1, section);
    let steel = steel();
    let mut heat = MemberHeat::new(&s, &steel, DVec3::ZERO);
    let fire = heat.fire(&s, &[0], iso_834(0.0));
    let dt = 1.0;
    let (am_v, rho) = (section.perimeter / section.area, steel.density);
    let mut en = 20.0;
    for k in 1..=3600 {
        let t = f64::from(k) * dt;
        // EN's explicit step, with the gas at the step's start.
        let gas = iso_834(t - dt) - 273.15;
        let net = 25.0 * (gas - en)
            + 0.7 * STEFAN_BOLTZMANN * (fourth(gas + 273.15) - fourth(en + 273.15));
        en += am_v / (steel.specific_heat.at(en + 273.15) * rho) * net * dt;
        // Backward Euler: the gas at the step's end.
        heat.set_fire(fire, iso_834(t));
        heat.step(dt, &mut s, None);
        if k % 900 == 0 {
            let ours = heat.temperature(0) - 273.15;
            assert!(
                (ours - en).abs() < 2.0,
                "{} min: {ours:.1} °C vs EN {en:.1} °C",
                k / 60
            );
        }
    }
    // After an hour in the fire the tube is near the gas's 945 °C.
    assert!((900.0..960.0).contains(&en), "{en}");
}

/// Heat flows along a member from where a fire touches it: the far end warms after the near.
#[test]
fn heat_flows_along_a_structure() {
    let mut s = cantilever(6, Section::solid_round(0.05));
    let mut heat = MemberHeat::new(&s, &steel(), DVec3::Y * -9.81);
    heat.in_air(&s, 293.15, 101_325.0, 293.15);
    heat.fire(&s, &[0], 1000.0);
    for _ in 0..600 {
        heat.step(1.0, &mut s, None);
    }
    let t: Vec<f64> = (0..6).map(|k| heat.temperature(k)).collect();
    assert!(t.windows(2).all(|w| w[0] > w[1]), "{t:?}");
    // In ten minutes heat diffuses √(α·t) ≈ 9 cm into steel: the far end, 2.5 m off, has
    // only just begun to warm.
    assert!(t[5] > 293.15 && t[5] < 293.2 && t[0] > 700.0, "{t:?}");
}

/// A cantilever heated to 600 °C deflects under its load 1/0.31 as far as cold (EN 1993-1-2
/// k_E), and grows by its thermal strain: the network's temperatures reach the structure.
#[test]
fn a_heated_structure_softens_and_lengthens() {
    let mut s = cantilever(4, Section::round_tube(0.1, 0.005));
    s.load[4] = DVec3::Y * -500.0;
    let (cold, _) = statics::linear(&s).unwrap();
    let mut heat = MemberHeat::new(&s, &steel(), DVec3::ZERO);
    let fire = heat.fire(&s, &[0, 1, 2, 3], 873.15);
    // Long enough for the thin tube to reach the fire's temperature.
    for _ in 0..200 {
        heat.step(10.0, &mut s, None);
    }
    heat.set_fire(fire, 873.15);
    assert!(
        (heat.temperature(2) - 873.15).abs() < 0.5,
        "{}",
        heat.temperature(2)
    );
    let (hot, _) = statics::linear(&s).unwrap();
    assert!(
        (hot[4].y / cold[4].y * 0.31 - 1.0).abs() < 0.01,
        "{}",
        hot[4].y / cold[4].y
    );
}

/// A hot member split in two (as buckling splits them) is followed by the network: both
/// halves as hot as it was, exposed as it was, and not a joule lost or made.
#[test]
fn a_split_member_is_followed() {
    let mut s = cantilever(3, Section::round_tube(0.1, 0.005));
    let mut heat = MemberHeat::new(&s, &steel(), DVec3::ZERO);
    let fire = heat.fire(&s, &[1], 1073.15);
    for _ in 0..30 {
        heat.step(10.0, &mut s, None);
    }
    let hot = heat.temperature(1);
    s.split_member(1);
    heat.step(1e-9, &mut s, None);
    assert_eq!(heat.members(), 4);
    assert!((heat.temperature(1) - hot).abs() < 1e-3 && (heat.temperature(3) - hot).abs() < 1e-3);
    // Both halves are still in the fire: they keep heating alike.
    heat.set_fire(fire, 1173.15);
    for _ in 0..10 {
        heat.step(10.0, &mut s, None);
    }
    assert!((heat.temperature(1) - heat.temperature(3)).abs() < 1.0);
    assert!(heat.temperature(1) > hot);
    let network = heat.network();
    assert!(network.ledger().relative_imbalance(network.stored()) < 1e-10);
}

fn fourth(x: f64) -> f64 {
    x * x * x * x
}
