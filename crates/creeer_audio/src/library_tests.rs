//! The shipped sound data loads, every entry is in range, and what it names is there.

use crate::library::Library;

#[test]
fn the_shipped_sound_data_loads_and_names_what_is_there() {
    let lib = Library::base();
    assert!(!lib.matters.is_empty());
    // Every engineering material sounds, those with entries by them.
    let spruce = lib.matter("spruce_c24").unwrap();
    assert!((spruce.acoustic.loss - 0.008).abs() < 1e-12);
    assert!(spruce.creak.is_some());
    let granite = lib.matter("granite").unwrap();
    assert!(
        (granite.acoustic.loss - 0.005).abs() < 1e-12,
        "the solver's damping"
    );
    // Fasteners driven by what the data says.
    let nail = lib.driver("nail_75").unwrap();
    assert_eq!(nail.blows, 5);
    assert!(lib.driver("bast_lashing").is_none());
    // The fasteners named are the catalogue's (data/fasteners.ron).
    let fasteners =
        std::fs::read_to_string(creeer_data::locate::source_tree().join("fasteners.ron")).unwrap();
    for d in &lib.drivers {
        assert!(
            fasteners.contains(&format!("name: \"{}\"", d.fastener)),
            "{}",
            d.fastener
        );
    }
    // The grounds named are rocks (data/geology/rocks.ron), but for the default.
    let rocks =
        std::fs::read_to_string(creeer_data::locate::source_tree().join("geology/rocks.ron"))
            .unwrap();
    for g in lib.grounds.iter().filter(|g| g.rock != "default") {
        assert!(
            rocks.contains(&format!("name: \"{}\"", g.rock)),
            "{}",
            g.rock
        );
    }
    assert_eq!(lib.ground("basalt").rock, "default");
    assert_eq!(lib.ground("gravel").rock, "gravel");
}

#[test]
fn a_body_is_known_by_its_stiffness_and_density() {
    let lib = Library::base();
    assert_eq!(lib.nearest(210e9, 7850.0).id, "steel_s235");
    assert_eq!(lib.nearest(50e9, 2650.0).id, "granite");
    assert!(lib.nearest(11e9, 450.0).id.starts_with("spruce"));
}
