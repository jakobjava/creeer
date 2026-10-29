//! The engineer holding things on the style board (G2.10, ADR-0036), animated as the game
//! animates him, to judge the drawn holding by (`--style-board <file> --holding <sheet.png>`):
//! a box under its bottom edges at the elbows, a pole across the body, a stick in one hand, and a
//! box held where it lies on the ground before his feet, he stooped to it.

use crate::animate::grasp::Load;
use crate::demo::DemoMeshes;
use crate::people::{DrawnPerson, People};
use crate::style_people::{Flat, look, side_by_side};
use creeer_math::Frame64;
use creeer_math::glam::{DQuat, DVec3, Vec3};
use creeer_render::{Camera, DebugLines, Frame, Image, Instance, Lighting, Material, Renderer};
use creeer_render::{Scene, Sky};
use creeer_sim::carry;
use creeer_sim::character::{self, bones};

/// The engineer on the board's slab beside its crates, facing +z.
fn centre() -> DVec3 {
    DVec3::new(4.5, character::heights().0, 0.0)
}

/// What he holds in each frame: a load whose point `grip` (its frame) is held at `at`.
fn loads() -> [Load; 4] {
    let held = carry::held(centre(), DVec3::Y, DVec3::Z);
    let load = |half: DVec3, grip: DVec3, at: DVec3, turn: DQuat, one_hand: bool, id: u32| Load {
        id,
        frame: Frame64::new(at - turn * grip, turn),
        centre: DVec3::ZERO,
        half,
        grip,
        one_hand,
    };
    let forward = DQuat::from_rotation_y(-std::f64::consts::FRAC_PI_2);
    [
        load(
            DVec3::splat(0.14),
            DVec3::Y * 0.14,
            held,
            DQuat::IDENTITY,
            false,
            1,
        ),
        load(
            DVec3::new(1.0, 0.03, 0.03),
            DVec3::Y * 0.03,
            held,
            DQuat::IDENTITY,
            false,
            2,
        ),
        load(
            DVec3::new(0.6, 0.015, 0.015),
            DVec3::new(-0.45, 0.015, 0.0),
            held - DVec3::X * 0.15,
            forward,
            true,
            3,
        ),
        load(
            DVec3::splat(0.14),
            DVec3::Y * 0.14,
            DVec3::new(centre().x, 0.28, 0.45),
            DQuat::IDENTITY,
            false,
            4,
        ),
    ]
}

/// The sheet: each load held for a second (the arms reaching it, the body stooping), seen from
/// his right front.
pub fn holding_sheet(
    renderer: &mut Renderer,
    people: &mut People,
    meshes: &DemoMeshes,
    (sky, lighting): (&Sky, &Lighting),
    board: &Scene,
) -> Image {
    let mut images = Vec::new();
    // The four standing; then the box and the stick carried walking at a walk's pace (1.4 m/s),
    // seen mid-stride (G2.10, the user: "when moving forward the arm goes through the body").
    let frames = (0..4).map(|k| (k, 0.0)).chain([(0, 1.4), (2, 1.4)]);
    for (n, (k, speed)) in frames.enumerate() {
        let start = loads()[k];
        let mut person = DrawnPerson {
            key: 100 + u32::try_from(n).unwrap_or(0),
            look: look(&bones::STARTING_OUTFIT),
            body: crate::animate::Body::Standing(centre(), DQuat::IDENTITY),
            wields: None,
            swing: None,
            holding: Some(start),
            made: None,
        };
        let mut scene = board.clone();
        scene.characters.clear();
        let (mut load, mut walked) = (start, DVec3::ZERO);
        for f in 0..=75 {
            walked = DVec3::Z * (speed * f64::from(f) / 60.0);
            load.frame = Frame64::new(start.frame.position + walked, start.frame.rotation);
            person.body = crate::animate::Body::Standing(centre() + walked, DQuat::IDENTITY);
            person.holding = Some(load);
            let mut warm = Scene::default();
            let into = if f == 75 { &mut scene } else { &mut warm };
            people.draw(std::slice::from_ref(&person), 1.0 / 60.0, &Flat, into);
        }
        #[allow(clippy::cast_possible_truncation)] // metres, to the GPU's f32
        let size = (load.half * 2.0).as_vec3();
        scene.add(
            meshes.cube,
            Instance {
                frame: load.frame * Frame64::from_position(load.centre),
                scale: Vec3::new(size.x, size.y, size.z),
                // Sawn spruce (`art-style.md` §3.2, `timber`).
                material: Material::dielectric([0.784, 0.631, 0.416], 0.7),
                heat: None,
            },
        );
        // Between his chest and what he holds.
        let look_at = 0.5 * (centre() + walked + DVec3::Y * 0.3 + load.held());
        let at = look_at + DVec3::new(-1.9, 0.35, 2.2);
        let d = look_at - at;
        let mut camera = Camera::new(at);
        camera.yaw = creeer_math::det::atan2(-d.x, -d.z);
        camera.pitch = creeer_math::det::asin(d.y / d.length());
        camera.fov_y = 40f32.to_radians();
        images.push(renderer.capture(Frame {
            camera: &camera,
            scene: &scene,
            terrain: &[],
            terrain_frames: &[],
            lighting,
            lines: &DebugLines::default(),
            sky: Some(sky),
            heatmap: None,
            overlay: None,
            clear_color: creeer_render::wgpu::Color::BLACK,
        }));
    }
    side_by_side(&images)
}
