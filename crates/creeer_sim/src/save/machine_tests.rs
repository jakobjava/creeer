//! Version 7: machines kept as their abstract selves.

use super::tests::parked_scene;
use super::*;
use crate::abstraction::snapshot::{Snapshot, StretchNumbers};
use creeer_math::glam::DVec3;

const V7: &[u8] = include_bytes!("fixtures/v7_f7_world.bin");

/// The parked scene and two machines: one abstract mid-stretch, one active.
fn machine_scene() -> Save {
    let mut save = parked_scene();
    let stretch = StretchNumbers {
        rates: vec![-0.0116, 0.0116, 0.0057],
        trends: vec![(0, -1e-7, 0.0), (0, 1e-7, 0.0), (1, -3e-4, 7.25)],
        at: 1405.0,
        began: 1420.0,
        ends: 1780.5,
    };
    let abstract_one = Snapshot {
        abstract_mode: true,
        time: 1500.0,
        amounts: vec![120.5, 59.5, 7.25],
        stretch: Some(stretch),
        measured: Some((
            1405.0,
            vec![-0.0116, 0.0116, 0.0057],
            vec![121.4, 58.6, 6.8],
        )),
        scale: vec![0.0116, 0.0116, 0.0057],
        fast: vec![1, 2, 3, 4],
    };
    let active = Snapshot {
        abstract_mode: false,
        time: 30.0,
        amounts: vec![150.0, 30.0, 0.0],
        stretch: None,
        measured: None,
        scale: Vec::new(),
        fast: Vec::new(),
    };
    save.machines = vec![
        SavedMachine {
            id: 1,
            kind: "powerhouse".into(),
            at: DVec3::new(1e5, 2e5, -3e5),
            born: 12,
            snapshot: abstract_one,
        },
        SavedMachine {
            id: 4,
            kind: "powerhouse".into(),
            at: DVec3::new(-1.5, 0.25, 4e5),
            born: 900,
            snapshot: active,
        },
    ];
    save
}

#[test]
#[ignore = "writes the version 7 fixture"]
fn write_v7_fixture() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/save/fixtures/v7_f7_world.bin"
    );
    std::fs::write(path, machine_scene().to_bytes()).unwrap();
}

#[test]
fn the_version_7_fixture_keeps_its_machines() {
    let save = Save::from_bytes(V7).unwrap();
    assert_eq!(save.to_bytes(), machine_scene().to_bytes());
    assert_eq!(save.machines, machine_scene().machines);
    // Damaged in its machines: refused.
    for cut in [V7.len() - 1, V7.len() - 40] {
        assert!(Save::from_bytes(&V7[..cut]).is_err(), "cut at {cut}");
    }
}
