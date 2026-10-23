//! Version 11 (G1.7): what was done to the trees kept exactly; an earlier save has every tree
//! whole.

use crate::save::tests::scene;
use crate::save::{Save, SavedTrees};
use crate::trees::{Cut, Part};

const V11: &[u8] = include_bytes!("fixtures/v11_g1_world.bin");

/// The fixtures' scene with a tree cut into, one felled, its part the scene's first body (on its
/// hinge, its second node's branches limbed), and a cut across it begun.
fn trees_scene() -> Save {
    let mut save = scene();
    let first = save.entities[0].0;
    let mut cut = Cut::at(0.6);
    cut.chop(0.3, 0.002);
    cut.chop(2.9, 0.0005);
    save.trees = SavedTrees {
        cuts: vec![((1 << 62) | 5, cut.clone())],
        felled: vec![((1 << 62) | 9, 0.55)],
        parts: vec![(
            first,
            Part {
                species: 1,
                seed: 0x1234_5678_9abc,
                dbh: 0.27,
                from: 0.55,
                to: 21.3,
                cut: vec![0, 2, 1],
            },
        )],
        hinged: vec![first],
        trunks: 0,
        bucks: vec![(first, cut)],
        limbing: vec![(first, 3, 1.5e-5)],
        cells: vec![0x1234_5678, 0x2_0000_0005],
        pending: vec![0x77],
        edits: 2,
    };
    save
}

#[test]
#[ignore = "writes the version 11 fixture"]
fn write_v11_fixture() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/save/fixtures/v11_g1_world.bin"
    );
    std::fs::write(path, trees_scene().to_bytes()).unwrap();
}

#[test]
fn the_version_11_fixture_keeps_its_trees_exactly() {
    let save = Save::from_bytes(V11).unwrap();
    assert_eq!(save.to_bytes(), V11);
    assert_eq!(save.trees, trees_scene().trees);
    // Saves of earlier versions have every tree whole.
    assert!(
        Save::from_bytes(&scene().to_bytes())
            .unwrap()
            .trees
            .is_empty()
    );
    for cut in [V11.len() - 1, V11.len() - 20] {
        assert!(Save::from_bytes(&V11[..cut]).is_err(), "cut at {cut}");
    }
}
