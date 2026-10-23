//! Saves read back as written, files of every earlier version read as they are now, and
//! what is no save, a later version's or damaged is refused.
//!
//! The fixtures are saves as each version wrote them: `v1_f3_edits.bin` byte by byte from
//! the F3 edit log's format (two edits), `v2_f5_world.bin` by F5, `v3_f6_world.bin` by F6.2,
//! `v4_f6_world.bin` by F6.4 and `v5_f6_world.bin` by this version (the scene below after half
//! a second; `cargo test -p creeer_sim -- --ignored write_v5_fixture` writes it again, which
//! only a new version may need). The scene's motion is the physics' of the version that wrote
//! it: v2 and v3 hold the same one (nothing in it is heated), v4 the one after F6.4 carried
//! rotations as momentum (`creeer_physics::structure::step`), v5 the one after F6 solved
//! rigid bodies' contacts by Newton's method proper and kept the energy they held. A change to how a version is written that
//! would not read its fixture fails here: such a change is a new version, with a migration.

use super::*;
use crate::avatar;
use creeer_math::glam::DVec3;
use creeer_physics::structure::Section;
use creeer_physics::structure::section::STEEL_S235;
use creeer_physics::structure::shapes::{Base, space_frame};
use creeer_physics::world::Body;
use creeer_physics::world::shape::{Shape, StaticShape};
use creeer_terrain::edits::{Brush, Edit};

const V1: &[u8] = include_bytes!("fixtures/v1_f3_edits.bin");
const V2: &[u8] = include_bytes!("fixtures/v2_f5_world.bin");
const V3: &[u8] = include_bytes!("fixtures/v3_f6_world.bin");
const V4: &[u8] = include_bytes!("fixtures/v4_f6_world.bin");
const V5: &[u8] = include_bytes!("fixtures/v5_f6_world.bin");
const V6: &[u8] = include_bytes!("fixtures/v6_f7_world.bin");
const DT: f64 = 1.0 / 60.0;

/// A crate, a ball, a player's avatar and a steel frame on level ground, half a second in,
/// with a hole dug.
pub(super) fn scene() -> Save {
    let mut world = World::new(DT);
    world.statics.push(StaticShape::Plane {
        normal: DVec3::Y,
        offset: 0.0,
    });
    world.add(Body::rigid(
        Shape::Box {
            half: DVec3::splat(0.5),
        },
        200.0,
        DVec3::new(0.0, 1.0, 0.0),
        DT,
    ));
    world.add(Body::rigid(
        Shape::Sphere { radius: 0.4 },
        300.0,
        DVec3::new(3.0, 2.0, 0.0),
        DT,
    ));
    world.add(avatar::body(DVec3::new(-3.0, 0.5, 1.0), DT));
    let frame = space_frame(
        [2, 2, 2],
        1.0,
        Section::round_tube(0.05, 0.003),
        STEEL_S235,
        Base::Free,
    );
    let mut frame = Body::structure(frame, DT);
    for p in &mut frame.structure.position {
        *p += DVec3::new(5.0, 0.1, -2.0);
    }
    frame.solver.reset_history(&frame.structure);
    world.add(frame);
    for _ in 0..30 {
        avatar::steer(&mut world, 2, (DVec3::X * 2.0, false), DVec3::Y);
        world.step();
    }
    let mut edits = Edits::default();
    edits.push(Edit::Dig(Brush::Sphere {
        center: DVec3::new(10.0, 0.0, 10.0),
        radius: 2.0,
    }));
    Save {
        planet: "planets/home.ron".into(),
        tick: 30,
        next_id: 9,
        edits,
        world,
        entities: vec![(1, None), (2, None), (5, Some("ada".into())), (8, None)],
        water: Vec::new(),
        parked: Vec::new(),
        machines: Vec::new(),
        people: Vec::new(),
        ages: Vec::new(),
        items: crate::save::SavedItems::default(),
        trees: crate::save::SavedTrees::default(),
        pits: Vec::new(),
    }
}

/// The scene with its first two bodies parked as one island and its last alone, the water
/// moving with a floater of the first (F7.2: version 6).
pub(super) fn parked_scene() -> Save {
    let mut save = scene();
    let (bodies, _) = save.world.take_bodies(|i, _| i != 2);
    let mut bodies = bodies.into_iter();
    let mut next = |id: u32, water: Vec<f64>| ParkedBody {
        id,
        player: None,
        body: bodies.next().unwrap(),
        water,
    };
    save.parked = vec![
        vec![next(1, vec![12.5]), next(2, Vec::new())],
        vec![next(8, Vec::new())],
    ];
    save.entities = vec![(5, Some("ada".into()))];
    save
}

#[test]
#[ignore = "writes the version 6 fixture"]
fn write_v6_fixture() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/save/fixtures/v6_f7_world.bin"
    );
    std::fs::write(path, parked_scene().to_bytes()).unwrap();
}

/// Parked islands come back as they went: their bodies bit for bit, their entities, the water.
#[test]
fn the_version_6_fixture_keeps_parked_islands() {
    let save = Save::from_bytes(V6).unwrap();
    assert_eq!(save.to_bytes(), parked_scene().to_bytes());
    let sizes: Vec<usize> = save.parked.iter().map(Vec::len).collect();
    assert_eq!(sizes, [2, 1]);
    assert_eq!(save.parked[0][0].water, [12.5]);
    let scene = scene();
    let mut parked = World::new(DT);
    parked.bodies = save
        .parked
        .iter()
        .flatten()
        .map(|p| p.body.clone())
        .collect();
    let mut expected = World::new(DT);
    expected.bodies = vec![
        scene.world.bodies[0].clone(),
        scene.world.bodies[1].clone(),
        scene.world.bodies[3].clone(),
    ];
    assert_eq!(parked.to_bytes(), expected.to_bytes());
    // A parked entity's id may not repeat a live one's.
    let mut clash = parked_scene();
    clash.parked[1][0].id = 5;
    assert!(matches!(
        Save::from_bytes(&clash.to_bytes()),
        Err(SaveError::Damaged(_))
    ));
}

#[test]
#[ignore = "writes the version 5 fixture"]
fn write_v5_fixture() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/save/fixtures/v5_f6_world.bin"
    );
    std::fs::write(path, scene().to_bytes()).unwrap();
}

#[test]
fn a_save_reads_back_as_written() {
    let save = scene();
    let bytes = save.to_bytes();
    let read = Save::from_bytes(&bytes).unwrap();
    assert_eq!(read.to_bytes(), bytes);
    assert_eq!(read.entities, save.entities);
    assert_eq!(read.edits.log(), save.edits.log());
}

#[test]
fn what_was_dug_and_not_cut_reads_back() {
    let mut save = scene();
    save.pits = vec![(DVec3::new(10.0, 0.0, 12.0), 0.004)];
    let read = Save::from_bytes(&save.to_bytes()).unwrap();
    assert_eq!(read.pits, save.pits);
    // Without the section (earlier versions), nothing is owed.
    assert!(
        Save::from_bytes(&scene().to_bytes())
            .unwrap()
            .pits
            .is_empty()
    );
}

#[test]
fn a_version_1_save_reads_as_a_world_with_its_edits() {
    let save = Save::from_bytes(V1).unwrap();
    assert_eq!(save.planet, V1_PLANET);
    assert!(save.world.bodies.is_empty() && save.entities.is_empty());
    let dig = Edit::Dig(Brush::Sphere {
        center: DVec3::new(1000.0, 2000.0, -500.0),
        radius: 3.0,
    });
    let fill = Edit::Fill(
        Brush::Capsule {
            a: DVec3::new(10.0, 20.0, 30.0),
            b: DVec3::new(11.0, 21.0, 31.0),
            radius: 1.5,
        },
        5,
    );
    assert_eq!(save.edits.log(), &[dig, fill]);
    // And it is written as this version.
    let again = Save::from_bytes(&save.to_bytes()).unwrap();
    assert_eq!(again.edits.log(), save.edits.log());
}

#[test]
fn the_version_2_fixture_reads_as_it_was_saved() {
    let save = Save::from_bytes(V2).unwrap();
    assert_eq!(
        (save.planet.as_str(), save.tick, save.next_id),
        ("planets/home.ron", 30, 9)
    );
    assert_eq!(save.entities, scene().entities);
    assert_eq!(save.world.bodies.len(), 4);
    assert_eq!(save.edits.len(), 1);
    // The world in it is version 3's, bit for bit (the same motion: nothing in it is heated,
    // so version 3 adds nothing to it), and both are written as this version alike.
    let v3 = Save::from_bytes(V3).unwrap();
    assert_eq!(save.world.to_bytes(), v3.world.to_bytes());
    assert_eq!(save.to_bytes(), v3.to_bytes());
}

#[test]
fn the_version_3_fixture_reads_as_it_was_saved() {
    let save = Save::from_bytes(V3).unwrap();
    assert_eq!(save.entities, scene().entities);
    assert_eq!(save.world.bodies.len(), 4);
    // Written again as this version, it reads back the same.
    let again = Save::from_bytes(&save.to_bytes()).unwrap();
    assert_eq!(again.to_bytes(), save.to_bytes());
}

#[test]
fn the_version_4_fixture_reads_as_it_was_saved() {
    let save = Save::from_bytes(V4).unwrap();
    assert_eq!(save.entities, scene().entities);
    assert_eq!(save.world.bodies.len(), 4);
    // Written again as this version (no energy left in its contacts: the step after it
    // finds them anew), it reads back the same.
    let again = Save::from_bytes(&save.to_bytes()).unwrap();
    assert_eq!(again.to_bytes(), save.to_bytes());
}

#[test]
fn the_version_5_fixture_reads_as_it_was_saved() {
    let save = Save::from_bytes(V5).unwrap();
    assert_eq!(save.world.to_bytes(), scene().world.to_bytes());
    // Written as this version, it reads back as itself (with the version's additions:
    // version 8's empty joints).
    let again = save.to_bytes();
    assert_eq!(again[8..12], VERSION.to_le_bytes());
    let read = Save::from_bytes(&again).unwrap();
    assert_eq!(read.to_bytes(), again);
    assert_eq!(read.world.to_bytes(), scene().world.to_bytes());
}

#[test]
fn what_is_no_save_a_later_one_or_a_damaged_one_is_refused() {
    assert_eq!(Save::from_bytes(b"hello").unwrap_err(), SaveError::NotASave);
    let mut later = V2.to_vec();
    later[8..12].copy_from_slice(&(VERSION + 1).to_le_bytes());
    assert_eq!(
        Save::from_bytes(&later).unwrap_err(),
        SaveError::Newer(VERSION + 1)
    );
    for cut in (9..V2.len()).step_by(211) {
        assert!(
            matches!(Save::from_bytes(&V2[..cut]), Err(SaveError::Damaged(_))),
            "cut at {cut}"
        );
    }
    for cut in (9..V1.len()).step_by(7) {
        assert!(
            matches!(Save::from_bytes(&V1[..cut]), Err(SaveError::Damaged(_))),
            "cut at {cut}"
        );
    }
}

#[test]
fn a_section_of_a_later_version_is_skipped() {
    let bytes = scene().to_bytes();
    // One more section, with a tag this version does not know.
    let mut w = Writer::new();
    w.raw(&bytes[..12]);
    let mut r = Reader::new(&bytes[12..]);
    let n = r.varint().unwrap();
    w.varint(n + 1);
    w.raw(b"NEWS");
    w.bytes_with_length(b"what a later game keeps");
    w.raw(&bytes[12 + (bytes.len() - 12 - r.remaining())..]);
    let read = Save::from_bytes(&w.finish()).unwrap();
    assert_eq!(read.to_bytes(), bytes);
}

/// The water moving with floating shapes comes back as it went (an optional section: a save
/// without it reads as before, the water still).
#[test]
fn the_water_moving_with_floaters_is_kept() {
    let mut save = scene();
    save.water = vec![0.0, 128.25, 3.5e-7];
    let back = Save::from_bytes(&save.to_bytes()).expect("reads");
    assert_eq!(back.water, save.water);
    let plain = scene();
    assert!(
        Save::from_bytes(&plain.to_bytes())
            .expect("reads")
            .water
            .is_empty()
    );
}
