//! Drawing physics snapshots.

use crate::DemoMeshes;
use crate::demo::Overlay;
use crate::physics_sandbox::PhysicsSnapshot;
use crate::physics_view::draw;
use creeer_math::glam::{DQuat, DVec3};
use creeer_render::{MeshArena, Scene};

#[test]
fn a_snapshot_without_looks_draws_crates_and_the_1_m_crate_as_its_model() {
    let mut arena = MeshArena::default();
    let meshes = DemoMeshes::register(&mut arena);
    let crate_model = meshes
        .models
        .crate_box
        .expect("the base pack has the crate");
    // As the planet's snapshot gives them: boxes and members, no looks, no temperatures.
    let mut snapshot = PhysicsSnapshot::default();
    let centre = DVec3::new(3.0, 0.5, -2.0);
    snapshot
        .boxes
        .push((centre, DQuat::IDENTITY, DVec3::splat(0.5)));
    snapshot
        .boxes
        .push((DVec3::ZERO, DQuat::IDENTITY, DVec3::splat(0.3)));
    snapshot
        .beams
        .push((DVec3::ZERO, DVec3::new(0.0, 2.0, 0.0), 0.05, 0.0, false));
    let mut scene = Scene::default();
    draw(
        &mut scene,
        &meshes,
        DVec3::ZERO,
        (&snapshot, &snapshot, 1.0),
        Overlay::Off,
    );
    let crates = scene.instances_of(crate_model);
    assert_eq!(crates.len(), 1);
    // The model's origin is its base's centre: half a metre below the body's centre.
    assert!((crates[0].frame.position - (centre - DVec3::Y * 0.5)).length() < 1e-12);
    assert_eq!(scene.instances_of(meshes.cube).len(), 1, "the 0.6 m box");
    assert_eq!(scene.instances_of(meshes.cylinder).len(), 1, "the member");
}
