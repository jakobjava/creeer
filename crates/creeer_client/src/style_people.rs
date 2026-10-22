//! The engineer on the style board (G1.3e–g): standing in the base layer and in the starting
//! outfit, fallen (a ragdoll let fall and come to rest), and a sheet of a walk cycle
//! (`--style-board <file> --walk <sheet.png>`), animated as the game animates him.

use crate::animate::feet::Ground;
use crate::animate::{Animator, Body, gait};
use crate::people::People;
use creeer_math::Frame64;
use creeer_math::glam::{DQuat, DVec3};
use creeer_net::entity::Look;
use creeer_physics::world::World;
use creeer_physics::world::shape::StaticShape;
use creeer_render::{Camera, Character, DebugLines, Frame, Image, Lighting, Renderer, Scene, Sky};
use creeer_sim::character::{self, bones, ragdoll};

/// The board's ground: the plane y = 0.
struct Flat;

impl Ground for Flat {
    fn under(&self, at: DVec3, _up: DVec3) -> Option<(DVec3, DVec3)> {
        Some((DVec3::new(at.x, 0.0, at.z), DVec3::Y))
    }
    fn depth(&self, _at: DVec3) -> f64 {
        f64::NEG_INFINITY
    }
}

/// How the engineer looks on the board: the first player colour, a middle skin tone, brown
/// hair, wearing `outfit`.
fn look(outfit: &[&str]) -> Look {
    Look {
        colour: 0,
        skin: 4,
        hair: 2,
        outfit: outfit.iter().map(|&w| w.to_owned()).collect(),
    }
}

/// The board's engineers: standing in the base layer and in the starting outfit behind the
/// crates, and one fallen in front of them.
pub fn add(people: &People, scene: &mut Scene) {
    for (x, outfit) in [
        (-0.8, &["base_layer"][..]),
        (0.8, &bones::STARTING_OUTFIT[..]),
    ] {
        let centre = DVec3::new(x, character::heights().0, -1.4);
        let standing = bones::standing_bones(centre, DQuat::from_rotation_y(-0.3 * x));
        scene
            .characters
            .push(people.character(&look(outfit), &standing));
    }
    let lying = fallen(DVec3::new(0.6, 0.0, 1.3));
    scene
        .characters
        .push(people.character(&look(&bones::STARTING_OUTFIT), &lying));
}

/// The ten skin tones side by side in the base layer (G1.3i), lightest on the left, in a scene
/// of their own over `ground`'s slab.
pub fn add_tones(people: &People, scene: &mut Scene) {
    for skin in 0..10u8 {
        let x = (f64::from(skin) - 4.5) * 0.62;
        let centre = DVec3::new(x, character::heights().0, -1.4);
        let standing = bones::standing_bones(centre, DQuat::IDENTITY);
        let look = Look {
            skin,
            ..look(&["base_layer"])
        };
        scene.characters.push(people.character(&look, &standing));
    }
}

/// A ragdoll fallen from standing, tipped over, at rest on the plane, its feet near `at`.
fn fallen(at: DVec3) -> Vec<Frame64> {
    let dt = 1.0 / 60.0;
    let mut world = World::new(dt);
    world.statics.push(StaticShape::Plane {
        normal: DVec3::Y,
        offset: 0.0,
    });
    let mut body = character::body(at, DVec3::Y, dt);
    body.structure.orientation[0] = DQuat::from_rotation_z(0.5) * DQuat::from_rotation_y(0.9);
    body.solver.reset_history(&body.structure);
    let index = world.add(body);
    let parts = ragdoll::fall(&mut world, index);
    for _ in 0..240 {
        world.step();
    }
    let poses: Vec<(DVec3, DQuat)> = (parts.iter())
        .map(|&i| {
            let s = &world.bodies[i].structure;
            (s.position[0], s.orientation[0])
        })
        .collect();
    bones::fallen_bones(&poses)
}

/// Renders eight frames of one stride of the engineer walking at 1.4 m/s, seen from his side,
/// as one sheet.
pub fn walk_sheet(
    renderer: &mut Renderer,
    people: &People,
    (sky, lighting): (&Sky, &Lighting),
    board: &Scene,
) -> Image {
    const FRAMES: usize = 8;
    let speed = 1.4;
    let stride = 2.0 / gait::at(speed).cadence;
    let dt = 1.0 / 120.0;
    let mut animator = Animator::default();
    let look = look(&bones::STARTING_OUTFIT);
    let mut t = 0.0;
    let mut images = Vec::new();
    // Walked three seconds first, into his stride.
    let mut next = 3.0;
    while images.len() < FRAMES {
        // Across the board's slab, beside its crates.
        let centre = DVec3::new(4.5, character::heights().0, speed * t - 3.5);
        let bones = animator.bones(1, &Body::Standing(centre, DQuat::IDENTITY), dt, &Flat);
        if t >= next {
            next += stride / f64::from(u32::try_from(FRAMES).unwrap_or(8));
            let mut camera = Camera::new(DVec3::new(centre.x + 3.2, 1.0, centre.z));
            camera.yaw = std::f64::consts::FRAC_PI_2;
            let mut scene = board.clone();
            scene.characters.clear();
            let drawn: Character = people.character(&look, &bones);
            scene.characters.push(drawn);
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
        t += dt;
    }
    side_by_side(&images)
}

/// The middle `CROP` pixels of images of one size, side by side.
fn side_by_side(images: &[Image]) -> Image {
    const CROP: u32 = 420;
    let (full, h) = (images[0].width, images[0].height);
    let (w, left) = (CROP.min(full), (full - CROP.min(full)) / 2);
    let count = u32::try_from(images.len()).expect("few frames");
    let mut rgba = vec![0u8; (w * count * h * 4) as usize];
    for (k, image) in images.iter().enumerate() {
        for y in 0..h as usize {
            let row = y * full as usize * 4 + left as usize * 4;
            let from = &image.rgba[row..row + w as usize * 4];
            let at = (y * (w * count) as usize + k * w as usize) * 4;
            rgba[at..at + w as usize * 4].copy_from_slice(from);
        }
    }
    Image {
        width: w * count,
        height: h,
        rgba,
    }
}
