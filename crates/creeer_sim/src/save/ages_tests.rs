//! Version 9 (G1.3c): each entity's age, its weathering and wear, kept exactly.

use super::tests::scene;
use super::*;
use crate::ageing::{AIR, PACE, SEA, Weathering};

const V9: &[u8] = include_bytes!("fixtures/v9_g1_world.bin");

/// The fixtures' scene with an age for each of its entities: new, a year in the air, worn,
/// and in the sea since tick 10.
fn aged_scene() -> Save {
    let mut save = scene();
    save.ages = vec![
        (1, Weathering::new(30, PACE * AIR)),
        (
            2,
            Weathering {
                exposure: 1.0,
                rate: PACE * AIR,
                since: 0,
                wear: 0.0,
            },
        ),
        (
            5,
            Weathering {
                exposure: 0.25,
                rate: PACE * AIR,
                since: 12,
                wear: 0.625,
            },
        ),
        (
            8,
            Weathering {
                exposure: 3.5,
                rate: PACE * SEA,
                since: 10,
                wear: 0.125,
            },
        ),
    ];
    save
}

#[test]
#[ignore = "writes the version 9 fixture"]
fn write_v9_fixture() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/save/fixtures/v9_g1_world.bin"
    );
    std::fs::write(path, aged_scene().to_bytes()).unwrap();
}

#[test]
fn the_version_9_fixture_keeps_every_age_exactly() {
    let save = Save::from_bytes(V9).unwrap();
    // Re-saved, it is the current version of what the scene writes (version 10 on: the same
    // sections, its header the newer version).
    assert_eq!(save.to_bytes(), aged_scene().to_bytes());
    assert_eq!(save.ages, aged_scene().ages);
    // Saves of earlier versions have no ages: everything in them is new.
    assert!(
        Save::from_bytes(&scene().to_bytes())
            .unwrap()
            .ages
            .is_empty()
    );
    // Damaged in its ages: refused.
    for cut in [V9.len() - 1, V9.len() - 20] {
        assert!(Save::from_bytes(&V9[..cut]).is_err(), "cut at {cut}");
    }
}
