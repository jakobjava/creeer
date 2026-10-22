//! The validator over the base pack, the example mod, and packs broken in every way it
//! should catch.

use crate::content::validate;
use creeer_data::Data;
use creeer_materials::MaterialTable;
use creeer_materials::species::SpeciesTable;
use creeer_terrain::Planet;
use creeer_terrain::geology::rocks::RockTable;
use std::path::{Path, PathBuf};

fn repository(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

/// The base pack and the repository's test packs (the example mod).
fn with_example() -> Data {
    Data::open(&repository("data"), &[repository("tests/packs")]).expect("the example opens")
}

#[test]
fn the_base_pack_validates_with_only_nasas_two_known_steps() {
    let report = validate(&Data::base());
    assert!(report.ok(), "{:#?}", report.errors);
    // ADR-0013: SnS(cr)'s α→β transition and ALN(L)'s step, kept as NASA gives them.
    assert_eq!(report.warnings.len(), 2, "{:#?}", report.warnings);
    assert!(report.warnings.iter().any(|w| w.contains("SnS(cr)")));
    assert!(report.warnings.iter().any(|w| w.contains("ALN(L)")));
    let count = |kind| report.counts.iter().find(|(k, _)| *k == kind).map(|c| c.1);
    assert_eq!(count("species"), Some(1798));
    assert_eq!(count("materials"), Some(9));
    assert_eq!(count("planets"), Some(4));
    assert_eq!(count("systems"), Some(1));
    // The palette (G1.3a; the eyes' and the worn coverall's tokens since, G1.3f), with no
    // style warnings beyond its approved exceptions.
    assert_eq!(count("palette tokens"), Some(49));
    // The models (G1.3b): the crate and the engineer (G1.3f), its skeleton the ragdoll's and
    // its wearables its pieces (no errors above).
    assert_eq!(count("models"), Some(2));
}

#[test]
fn the_compiled_in_tables_are_the_base_packs() {
    let data = Data::base();
    assert_eq!(
        MaterialTable::from_data(&data).unwrap(),
        MaterialTable::reference()
    );
    assert_eq!(
        SpeciesTable::from_data(&data).unwrap(),
        SpeciesTable::reference()
    );
}

#[test]
fn the_example_mod_replaces_adds_and_validates() {
    let data = with_example();
    let report = validate(&data);
    assert!(report.ok(), "{:#?}", report.errors);
    assert_ne!(data.hash(), Data::base().hash());
    let materials = MaterialTable::from_data(&data).unwrap();
    assert_eq!(materials.all().len(), 10);
    // Replaced in place (same position), new ones after.
    let granite = materials.get("granite").unwrap();
    assert!((granite.youngs_modulus.at(20.0) - 60e9).abs() < 1.0);
    assert_eq!(materials.all()[8].id, "granite");
    assert_eq!(materials.all()[9].id, "basalt");
    // A new rock goes after the base's, so the base's rocks keep their terrain ids.
    let base_rocks = RockTable::from_data(&Data::base()).unwrap();
    let rocks = RockTable::from_data(&data).unwrap();
    assert_eq!(rocks.rocks.len(), base_rocks.rocks.len() + 1);
    assert_eq!(rocks.rocks.last().unwrap().name, "chromitite");
    for (a, b) in base_rocks.rocks.iter().zip(&rocks.rocks) {
        assert_eq!(a.name, b.name);
    }
    let moon = Planet::load(&data, "planets/example_moon.ron").unwrap();
    assert_eq!(moon.name, "Example moon");
    assert_eq!(data.origin("planets/example_moon.ron"), Some("example"));
}

/// The base pack plus one mod named `name` with `files`; the validator's errors.
fn broken(name: &str, files: &[(&str, &str)]) -> Vec<String> {
    let mods = std::env::temp_dir().join(format!("creeer_validate_{}_{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&mods);
    let root = mods.join(name);
    std::fs::create_dir_all(&root).unwrap();
    let manifest = format!("Pack(name: {name:?}, version: \"1\", requires: [\"base\"])");
    std::fs::write(root.join("pack.ron"), manifest).unwrap();
    for (path, text) in files {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
    let data = Data::open(&repository("data"), std::slice::from_ref(&mods)).unwrap();
    let report = validate(&data);
    let _ = std::fs::remove_dir_all(&mods);
    report.errors
}

fn one_error(errors: &[String], needles: &[&str]) {
    assert_eq!(errors.len(), 1, "{errors:#?}");
    for needle in needles {
        assert!(
            errors[0].contains(needle),
            "{needle:?} not in {:?}",
            errors[0]
        );
    }
}

#[test]
fn the_validator_catches_each_kind_of_broken_data() {
    // A unit of the wrong dimension.
    let material = std::fs::read_to_string(repository("tests/packs/example/materials/rocks.ron"))
        .unwrap()
        .replace("\"2900 kg/m^3\"", "\"2900 kg\"");
    one_error(
        &broken("units", &[("materials/rocks.ron", &material)]),
        &["units: materials/rocks.ron"],
    );
    // A rock of a mineral no pack has.
    let rock = r##"[(name: "fakeite", color: "#000000", porosity: 0.0, minerals: [("unobtainium", 1.0)])]"##;
    one_error(
        &broken("mineral", &[("geology/rocks.ron", rock)]),
        &["fakeite", "unobtainium"],
    );
    // A planet whose geology file is not there.
    let planet = std::fs::read_to_string(repository("data/planets/rocky.ron"))
        .unwrap()
        .replace("home_geology.ron", "lost_geology.ron");
    one_error(
        &broken("geology", &[("planets/lost.ron", &planet)]),
        &["planets/lost_geology.ron"],
    );
    // Hydrogen made so stable that water would fall apart by itself.
    let h2 = r#"[(name: "H2", atoms: [("H", 2.0)], phase: Gas, ranges: [200.0, 6000.0], polynomials: [[0.0, 0.0, 3.5, 0.0, 0.0, 0.0, 0.0, -1.0e6, 0.0]], note: "broken")]"#;
    one_error(
        &broken("hydrogen", &[("chemistry/h2.ron", h2)]),
        &["ΔG", "wrong sign"],
    );
    // A heat capacity that goes negative.
    let cold = r#"[(name: "Coldium", atoms: [("He", 1.0)], phase: Gas, ranges: [200.0, 1000.0], polynomials: [[0.0, 0.0, 2.5, -0.01, 0.0, 0.0, 0.0, 0.0, 0.0]], note: "broken")]"#;
    one_error(
        &broken("cold", &[("chemistry/cold.ron", cold)]),
        &["Coldium", "heat capacity"],
    );
    // A system naming a template no pack has.
    let system = std::fs::read_to_string(repository("data/systems/default.ron"))
        .unwrap()
        .replacen("planets/rocky.ron", "planets/missing.ron", 1);
    one_error(
        &broken("system", &[("systems/default.ron", &system)]),
        &["system: systems/default.ron", "planets/missing.ron"],
    );
    // A palette token with a malformed colour (G1.3a).
    let token = r##"[(name: "steel", group: Material, colour: Hex("#8a8d9"), metalness: 1.0)]"##;
    one_error(
        &broken("palette", &[("art/palette.ron", token)]),
        &["palette: art/palette.ron", "steel", "bad colour"],
    );
    // A model whose file is no glTF, and one whose file is not there (G1.3b).
    let manifest = r#"[(name: "junk", file: "models/junk.glb", class: Item, bounds: ((0.0, 0.0, 0.0), (1.0, 1.0, 1.0)))]"#;
    one_error(
        &broken(
            "junk",
            &[("models/models.ron", manifest), ("models/junk.glb", "text")],
        ),
        &["junk: models/junk.glb", "not a glTF"],
    );
    one_error(
        &broken("lost", &[("models/models.ron", manifest)]),
        &["models/junk.glb", "no pack has this file"],
    );
}

#[test]
fn a_mods_look_is_checked_against_the_style_as_warnings() {
    // Steel recoloured a saturated green: it loads, and the style's checks say why it is off.
    let mods = std::env::temp_dir().join(format!("creeer_validate_{}_look", std::process::id()));
    let _ = std::fs::remove_dir_all(&mods);
    let root = mods.join("look");
    std::fs::create_dir_all(root.join("art")).unwrap();
    std::fs::write(
        root.join("pack.ron"),
        "Pack(name: \"look\", version: \"1\", requires: [\"base\"])",
    )
    .unwrap();
    let token = r##"[(name: "steel", group: Material, colour: Hex("#40e040"), metalness: 1.0)]"##;
    std::fs::write(root.join("art/palette.ron"), token).unwrap();
    let data = Data::open(&repository("data"), std::slice::from_ref(&mods)).unwrap();
    let report = validate(&data);
    let _ = std::fs::remove_dir_all(&mods);
    assert!(report.ok(), "{:#?}", report.errors);
    let style: Vec<&String> = (report.warnings.iter())
        .filter(|w| w.starts_with("art style: steel"))
        .collect();
    assert!(style.iter().any(|w| w.contains("chroma")), "{style:#?}");
    // Base steel's exception against gravel is not needed by the green: it says so.
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("no longer needed")),
        "{:#?}",
        report.warnings
    );
}
