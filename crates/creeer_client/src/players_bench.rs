//! Stand-in players for the bench (G1.3f, ADR-0026): the engineer's levels of detail are
//! budgeted by measuring their draw before they are modelled. `--players <glb> <count>` with
//! `--bench` loads a stand-in mesh of a level's triangle count and draws `count` copies where
//! players would stand: the first 3 m in front of the camera (as close as a third-person
//! camera comes), the others in rows of four behind it, 1.5 m apart, all on the ground and
//! facing the camera, in the main pass and every shadow cascade they reach.

use creeer_art::model::Model;
use creeer_math::Frame64;
use creeer_math::glam::{DQuat, DVec3, Vec3};
use creeer_render::{Camera, Instance, Material, MeshArena, MeshData, MeshId, Scene};

/// The stand-in's look: `coverall`'s grey-blue fabric.
const LOOK: Material = Material::dielectric([0.55, 0.6, 0.65], 0.85);

/// Loads the stand-in at `path` into `arena`, its materials resolved against the packs'
/// palette.
///
/// # Errors
/// If the file does not read, the palette does not load, or the model is refused.
pub fn load(path: &str, arena: &mut MeshArena) -> Result<(MeshId, usize), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let palette = crate::models::palette().ok_or("the palette does not load")?;
    let model = Model::read(&bytes, &palette).map_err(|e| format!("{path}: {e}"))?;
    let triangles = model.indices.len() / 3;
    Ok((arena.add(&MeshData::from_model(&model)), triangles))
}

/// Adds `count` copies of `mesh` where players would stand before `camera`, on the ground at
/// `ground` (m, its height).
pub fn add(scene: &mut Scene, camera: &Camera, mesh: MeshId, count: usize, ground: f64) {
    for frame in places(camera, count, ground) {
        scene.add(
            mesh,
            Instance {
                frame,
                scale: Vec3::ONE,
                material: LOOK,
                heat: None,
            },
        );
    }
}

/// As [`add`], the engineer himself (G1.3e): `count` skinned characters in the starting
/// outfit, standing where the stand-ins stand.
pub fn add_people(
    scene: &mut Scene,
    camera: &Camera,
    people: &crate::people::People,
    count: usize,
    ground: f64,
) {
    for frame in places(camera, count, ground) {
        let person = crate::people::DrawnPerson {
            look: creeer_net::entity::Look {
                outfit: (creeer_sim::character::bones::STARTING_OUTFIT.iter())
                    .map(|&w| w.to_owned())
                    .collect(),
                ..Default::default()
            },
            bones: vec![frame; creeer_sim::character::bones::SEGMENT_NAMES.len()],
        };
        scene.characters.push(people.character(&person));
    }
}

/// Where `count` players stand, facing the camera: one 3 m ahead, the rest in rows of four
/// beyond, on the ground at `ground`.
fn places(camera: &Camera, count: usize, ground: f64) -> Vec<Frame64> {
    let forward = DVec3::new(camera.forward().x, 0.0, camera.forward().z).normalize_or_zero();
    let right = forward.cross(DVec3::Y);
    // The model faces +z: turned to face the camera, back along `forward`.
    let facing = DQuat::from_rotation_arc(DVec3::Z, -forward);
    let near = DVec3::new(camera.position.x, ground, camera.position.z) + forward * 3.0;
    (0..count)
        .map(|k| {
            #[allow(clippy::cast_precision_loss)]
            let (row, column) = ((k / 4) as f64, (k % 4) as f64);
            let at = if k == 0 {
                near
            } else {
                near + forward * (2.0 + row * 1.5) + right * ((column - 1.5) * 1.5)
            };
            Frame64::new(at, facing)
        })
        .collect()
}
