//! The style board (`docs/art-style.md` §11.5; begun in G1.3c, completed in G1.3h): models on
//! the grounds at their ages, rendered headless, to judge the look by.
//!
//! `creeer_client --style-board <file.png> [--ground soil|gravel|granite|basalt|grass]
//! [--sun <deg>]` draws the crate new, weathered a year and four years, side by side.

use crate::demo::{self, DemoMeshes};
use crate::people::DrawnPerson;
use creeer_math::Frame64;
use creeer_math::glam::{DQuat, DVec3, Vec3};
use creeer_net::entity::Look;
use creeer_render::{Camera, DebugLines, Frame, Instance, Lighting, Material, Renderer, Scene};
use creeer_sim::character::bones::{STARTING_OUTFIT, standing_bones};
use std::path::Path;

/// The five grounds (`art-style.md` §4) by name, sRGB.
const GROUNDS: [(&str, [u8; 3]); 5] = [
    ("soil", [0x6b, 0x4f, 0x35]),
    ("gravel", [0x8c, 0x81, 0x74]),
    ("granite", [0xc4, 0xa1, 0x94]),
    ("basalt", [0x3d, 0x3e, 0x44]),
    ("grass", [0x6b, 0x7a, 0x3f]),
];
/// The crates' weathering (years at the reference rate, `art-style.md` §6b).
const AGES: [f32; 3] = [0.0, 1.0, 4.0];

/// Renders the board described by `args` (after `--style-board`).
pub fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let value_of = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
    };
    let path = value_of("--style-board").ok_or("--style-board needs a file name")?;
    let ground = value_of("--ground").map_or("soil", String::as_str);
    let colour = GROUNDS
        .iter()
        .find(|(name, _)| *name == ground)
        .ok_or_else(|| {
            format!("--ground expects soil/gravel/granite/basalt/grass, got `{ground}`")
        })?
        .1;
    let sun: f64 = match value_of("--sun") {
        Some(s) => s
            .parse()
            .map_err(|_| format!("--sun expects degrees, got `{s}`"))?,
        None => 50.0,
    };

    let mut renderer = pollster::block_on(Renderer::new_headless((1600, 900)))?;
    let meshes = DemoMeshes::register_for(&mut renderer);
    let crate_model = meshes
        .models
        .crate_box
        .ok_or("the packs have no crate model")?;
    let mut scene = Scene::default();
    // The ground: a slab 12 m by 8 m, its top at y = 0.
    let slab = Material::dielectric(colour.map(|c| f32::from(c) / 255.0), 0.92);
    scene.add(
        meshes.cube,
        Instance {
            frame: Frame64::from_position(DVec3::new(0.0, -0.1, 0.0)),
            scale: Vec3::new(12.0, 0.2, 8.0),
            material: slab,
            heat: None,
        },
    );
    for (k, &age) in AGES.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)] // three crates
        let x = (k as f64 - 1.0) * 1.6;
        // A quarter turn between them, so each shows a different side.
        #[allow(clippy::cast_precision_loss)]
        let turn = DQuat::from_rotation_y(0.35 * k as f64);
        scene.add(
            crate_model,
            Instance {
                frame: Frame64::new(DVec3::new(x, 0.0, 0.0), turn),
                scale: Vec3::ONE,
                material: Material::dielectric([1.0; 3], 0.8).aged(age, 0.0),
                heat: None,
            },
        );
    }
    // The engineer (G1.3e) behind them: in his base layer alone, and in the starting outfit,
    // turned a little to the camera.
    let people = crate::people::People::register(&mut renderer)?;
    for (x, outfit) in [(-0.8, &["base_layer"][..]), (0.8, &STARTING_OUTFIT[..])] {
        let look = Look {
            colour: 0,
            skin: 4,
            hair: 2,
            outfit: outfit.iter().map(|&w| w.to_owned()).collect(),
        };
        let centre = DVec3::new(x, creeer_sim::character::heights().0, -1.4);
        let bones = standing_bones(centre, DQuat::from_rotation_y(-0.3 * x));
        scene
            .characters
            .push(people.character(&DrawnPerson { look, bones }));
    }
    // The board's view, or `--camera x y z yaw pitch` (m, degrees) for a close look.
    let mut camera = Camera::new(DVec3::new(0.0, 1.55, 3.1));
    camera.pitch = -0.3;
    if let Some(i) = args.iter().position(|a| a == "--camera") {
        let n: Vec<f64> = (args.iter().skip(i + 1).take(5))
            .map(|s| {
                s.parse()
                    .map_err(|_| format!("--camera expects numbers, got `{s}`"))
            })
            .collect::<Result<_, _>>()?;
        let [x, y, z, yaw, pitch] = n[..] else {
            return Err("--camera expects x y z yaw pitch".into());
        };
        camera = Camera::new(DVec3::new(x, y, z));
        (camera.yaw, camera.pitch) = (yaw.to_radians(), pitch.to_radians());
    }
    let sky = demo::sky(DVec3::ZERO);
    let lighting = sky.lighting(
        camera.position,
        demo::sun_direction(sun),
        &Lighting::default(),
    );
    let image = renderer.capture(Frame {
        camera: &camera,
        scene: &scene,
        terrain: &[],
        terrain_frames: &[],
        lighting: &lighting,
        lines: &DebugLines::default(),
        sky: Some(&sky),
        heatmap: None,
        overlay: None,
        clear_color: creeer_render::wgpu::Color::BLACK,
    });
    image.save_png(Path::new(path))?;
    println!("saved {path}: the crate at {AGES:?} years on {ground}, sun {sun}°");
    Ok(())
}
