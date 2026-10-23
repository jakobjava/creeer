use super::*;
use crate::strike;
use crate::tools::Tool;
use creeer_data::Data;
use creeer_terrain::geology::rocks::RockTable;

const G: f64 = 9.81;
/// Hardened tool steel (`tools.ron`) and a chert head's hardness from its minerals.
const STEEL: f64 = 6.0e9;

fn rocks() -> RockTable {
    RockTable::from_data(&Data::base()).expect("rocks")
}

/// A ground's mean hardness, specific energy and whether it is solid.
fn ground(rocks: &RockTable, name: &str) -> (f64, f64, bool) {
    let id = rocks.rock(name).expect(name);
    let r = &rocks.rocks[usize::from(id)];
    (
        mean_hardness(rocks.hardnesses(id)),
        excavation_energy(r.strength, r.density, G),
        matches!(r.strength, Strength::Solid { .. }),
    )
}

/// The catalogue's pickaxe's blow: its energy into ground that does not give (J).
fn pick_blow() -> f64 {
    let tools = Tool::all(&Data::base()).expect("tools");
    let s = strike::swing(&tools[0]);
    0.5 * s.mass * s.speed * s.speed
}

#[test]
fn every_rock_has_a_strength_and_its_minerals_a_hardness() {
    let rocks = rocks();
    for m in &rocks.minerals {
        assert!(m.hardness > 1e7 && m.hardness < 3e10, "{}", m.name);
    }
    for r in &rocks.rocks {
        let e = excavation_energy(r.strength, r.density, G);
        assert!(e > 1e3 && e < 5e8, "{}: {e} J/m³", r.name);
    }
}

#[test]
fn loose_soil_takes_a_tillage_tools_specific_draft() {
    // McKyes 1985: 20–100 kPa in loam.
    let (_, soil, solid) = ground(&rocks(), "soil");
    assert!(!solid);
    assert!((20e3..100e3).contains(&soil), "{soil}");
}

#[test]
fn a_blow_digs_by_the_specific_energy_relation() {
    let rocks = rocks();
    let energy = pick_blow();
    assert!((150.0..400.0).contains(&energy), "{energy} J");
    let chert = mean_hardness(rocks.hardnesses(rocks.rock("chert").expect("chert")));
    let mut volumes = Vec::new();
    for name in ["granite", "sandstone", "soil"] {
        let (hard, e_s, solid) = ground(&rocks, name);
        for (head, tip) in [(STEEL, 0.003), (chert, 0.003)] {
            let eta = efficiency(tip, head, hard, solid);
            let v = dug(energy, eta, e_s);
            // The law itself: the volume's specific energy is e_s / η.
            assert!((energy * eta / v - e_s).abs() < 1e-9 * e_s);
            volumes.push((name, head, v));
        }
    }
    let v = |name: &str, head: f64| {
        (volumes.iter())
            .find(|x| x.0 == name && x.1.to_bits() == head.to_bits())
            .map(|x| x.2)
            .expect("measured")
    };
    // A pick in soil: about a litre a blow; on granite under a cubic centimetre.
    assert!(
        (3e-4..3e-3).contains(&v("soil", STEEL)),
        "{}",
        v("soil", STEEL)
    );
    assert!(v("granite", STEEL) < 1e-6, "{}", v("granite", STEEL));
    // Weaker rock gives more; a chert head, harder than steel, breaks granite better.
    assert!(v("sandstone", STEEL) > 2.0 * v("granite", STEEL));
    assert!(v("granite", chert) > v("granite", STEEL));
    // In loose ground the head's hardness does not matter.
    assert!((v("soil", chert) - v("soil", STEEL)).abs() < 1e-15);
}

#[test]
fn a_blunt_face_breaks_less_than_a_point() {
    let sharp = efficiency(0.0, STEEL, 8e9, true);
    assert!((efficiency(BLUNT, STEEL, 8e9, true) - 0.5 * sharp).abs() < 1e-12);
    assert!(efficiency(0.04, STEEL, 8e9, true) < 0.06 * sharp);
}

#[test]
fn a_soft_head_barely_marks_hard_rock_and_wears_away() {
    let rocks = rocks();
    let granite = rocks.rock("granite").expect("granite");
    let (hard, e_s, _) = ground(&rocks, "granite");
    // Copper (annealed, 3 × 220 MPa) against granite's grains.
    let copper = 0.66e9;
    assert!(efficiency(0.003, copper, hard, true) < 0.05 * efficiency(0.003, STEEL, hard, true));
    let soft = wear(100.0, copper, rocks.hardnesses(granite), e_s);
    let steel = wear(100.0, STEEL, rocks.hardnesses(granite), e_s);
    assert!(soft > 5.0 * steel, "{soft} vs {steel}");
}

#[test]
fn a_stone_head_wears_faster_on_granite_than_on_sandstone() {
    let rocks = rocks();
    let chert = mean_hardness(rocks.hardnesses(rocks.rock("chert").expect("chert")));
    let wear_on = |name: &str, head: f64| {
        let id = rocks.rock(name).expect(name);
        let (_, e_s, _) = ground(&rocks, name);
        wear(100.0, head, rocks.hardnesses(id), e_s)
    };
    assert!(wear_on("granite", chert) > wear_on("sandstone", chert));
    // A steel pick's point loses about 0.2 mm³ a blow on granite (WEAR's calibration).
    let steel = wear_on("granite", STEEL) * pick_blow() / 100.0;
    assert!((0.05e-9..0.5e-9).contains(&steel), "{steel} m³");
    // Soil barely wears anything.
    assert!(wear_on("soil", STEEL) < 1e-3 * wear_on("granite", STEEL));
}

#[test]
fn abrasion_passes_between_its_regimes_continuously() {
    assert_eq!(abrasion(0.5, 1.0), SOFT);
    assert_eq!(abrasion(2.0, 1.0), 1.0);
    assert!((abrasion(0.8, 1.0) - SOFT).abs() < 1e-12);
    assert!((abrasion(1.2, 1.0) - 1.0).abs() < 1e-12);
    assert!(abrasion(1.0, 1.0) > SOFT && abrasion(1.0, 1.0) < 1.0);
}

#[test]
fn a_worn_edge_blunts_as_its_wedge_is_cut_back() {
    // A 30° edge 5 cm long: 1 mm³ lost leaves a flat sqrt(4·tan15°·1e-9/0.05) wide.
    let r = worn_tip(30f64.to_radians(), 0.05, 1e-9);
    let w = (4.0 * creeer_math::det::tan(15f64.to_radians()) * 1e-9 / 0.05).sqrt();
    assert!((r - 0.5 * w).abs() < 1e-15);
    assert_eq!(worn_tip(0.5, 0.05, 0.0), 0.0);
}

#[test]
fn a_dug_ores_composition_follows_its_grade() {
    let rocks = rocks();
    let ore = rocks.rock("porphyry copper ore").expect("ore");
    let mass =
        |c: &[(String, f64)], name: &str| (c.iter()).find(|(n, _)| n == name).map_or(0.0, |x| x.1);
    let average = rocks.composition(ore, 1.0);
    let core = rocks.composition(ore, 2.5);
    let rim = rocks.composition(ore, 0.0);
    for c in [&average, &core, &rim] {
        let total: f64 = c.iter().map(|x| x.1).sum();
        assert!((total - 1.0).abs() < 1e-12);
    }
    let (a, k) = (mass(&average, "chalcopyrite"), mass(&core, "chalcopyrite"));
    // 2.5 times the volume; the mass fraction a little less, the solid now denser.
    assert!(k > 2.3 * a && k < 2.5 * a, "{a} {k}");
    assert_eq!(mass(&rim, "chalcopyrite"), 0.0);
    // A rock without graded minerals keeps its own.
    let granite = rocks.rock("granite").expect("granite");
    assert_eq!(
        rocks.composition(granite, 2.0),
        rocks.composition(granite, 1.0)
    );
}

#[test]
fn a_blow_into_soil_loads_a_binding_less_than_one_on_rock() {
    let rocks = rocks();
    let energy = pick_blow();
    let impulse = (2.0 * energy * 2.0, 0.002);
    let force = |name: &str| {
        let (hard, e_s, solid) = ground(&rocks, name);
        let v = dug(energy, efficiency(0.003, STEEL, hard, solid), e_s);
        peak_force(energy, v, impulse)
    };
    // In soil the head goes in about a tool's bite: the energy over 10 cm, a few kN; a stone
    // axe's bark binding holds about 7 kN (ADR-0030).
    assert!(force("soil") < 4e3, "{} N", force("soil"));
    assert!(force("granite") > 10.0 * force("soil"));
}
