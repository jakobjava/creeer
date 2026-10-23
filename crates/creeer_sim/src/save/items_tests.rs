//! Version 10 (G1.6a): the bodies that are items, and avatars' pockets, kept exactly; an earlier
//! save has none.

use crate::items::{Bulk, Form, Item, Piece, Store};
use crate::save::tests::scene;
use crate::save::{Save, SavedItems};
use creeer_math::glam::DVec3;

const V10: &[u8] = include_bytes!("fixtures/v10_g1_world.bin");

/// The fixtures' scene with its first body a crate holding a stone and some sand, and a pocket
/// with a pebble in it for its second entity.
fn items_scene() -> Save {
    let mut save = scene();
    let mut crate_ = Piece::solid(
        "spruce_c24",
        420.0,
        Form::Block {
            size: DVec3::new(0.5, 0.4, 0.4),
        },
    );
    crate_.wall = 0.018;
    let stone = Piece::solid("granite", 2650.0, Form::Ball { radius: 0.04 });
    let sand = Bulk {
        mass: 2.5,
        composition: vec![("quartz".into(), 0.9), ("orthoclase".into(), 0.1)],
        temperature: 288.15,
        specific_heat: 830.0,
        grain: 0.0005,
        bulk_density: 1600.0,
    };
    let held = vec![Item::Piece(stone, Vec::new()), Item::Bulk(sand)];
    let mut pocket = Store::new("jacket.chest.L", [0.095, 0.115, 0.03]);
    let pebble = Piece::solid("granite", 2650.0, Form::Ball { radius: 0.015 });
    pocket.put(Item::Piece(pebble, Vec::new())).unwrap();
    let (first, second) = (save.entities[0].0, save.entities[1].0);
    save.items = SavedItems {
        bodies: vec![(first, Item::Piece(crate_, held))],
        pockets: vec![(second, vec![pocket])],
        taken: vec![(1 << 40) | 7, 3],
        laid: vec![(
            (1 << 63) | 2,
            Item::Piece(
                Piece::solid(
                    "chert",
                    2580.0,
                    Form::Lump {
                        size: DVec3::new(0.04, 0.03, 0.008),
                    },
                ),
                Vec::new(),
            ),
            DVec3::new(1.0, 2.0, 3.0),
            creeer_math::glam::DQuat::from_rotation_z(0.3),
        )],
    };
    save
}

#[test]
#[ignore = "writes the version 10 fixture"]
fn write_v10_fixture() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/save/fixtures/v10_g1_world.bin"
    );
    std::fs::write(path, items_scene().to_bytes()).unwrap();
}

#[test]
fn the_version_10_fixture_keeps_its_items_exactly() {
    let save = Save::from_bytes(V10).unwrap();
    assert_eq!(save.to_bytes(), V10);
    assert_eq!(save.items, items_scene().items);
    // Saves of earlier versions have no items.
    assert!(
        Save::from_bytes(&scene().to_bytes())
            .unwrap()
            .items
            .is_empty()
    );
    // Damaged in its items: refused.
    for cut in [V10.len() - 1, V10.len() - 20] {
        assert!(Save::from_bytes(&V10[..cut]).is_err(), "cut at {cut}");
    }
}
