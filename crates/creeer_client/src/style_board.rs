//! The style board (`docs/art-style.md` §11.5; begun in G1.3c, completed in G1.3h): every model
//! on the grounds, rendered headless, to judge the look by, and held to approved golden images.
//!
//! - `creeer_client --style-board <file.png> [--ground soil|gravel|granite|basalt|grass]
//!   [--sun <deg>] [--camera x y z yaw pitch] [--walk <sheet.png>] [--holding <sheet.png>]`: one
//!   board: the crate new, weathered a year and four years, the engineer in his base layer, in
//!   his outfit and fallen, a stride of him walking (`style_people.rs`), and him holding things
//!   (G2.10, `style_holding.rs`).
//! - `creeer_client --style-board <dir> --all [--check <golden dir> | --save <golden dir>]`:
//!   the board on each of the five grounds at noon and in the evening, the walk and run sheets
//!   (G1.4a) and the items' icons, written to `dir`; held to the golden images (`golden.rs`),
//!   or made them (an intended change, once the user has approved it). `cargo xtask style-board`
//!   runs it.

use crate::demo::{self, DemoMeshes};
use crate::people::People;
use creeer_math::Frame64;
use creeer_math::glam::{DQuat, DVec3, Vec3};
use creeer_render::{
    Camera, DebugLines, Frame, Image, Instance, Lighting, Material, Renderer, Scene,
};
use std::path::{Path, PathBuf};

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
/// The sun at noon and in the evening (degrees above the horizon).
const TIMES: [(&str, f64); 2] = [("noon", 60.0), ("evening", 10.0)];

/// A renderer with the board's models and people.
struct Board {
    renderer: Renderer,
    meshes: DemoMeshes,
    people: People,
}

impl Board {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let mut renderer = pollster::block_on(Renderer::new_headless((1600, 900)))?;
        let meshes = DemoMeshes::register_for(&mut renderer);
        let people = People::register(&mut renderer)?;
        Ok(Self {
            renderer,
            meshes,
            people,
        })
    }

    /// The ground alone: a slab 80 m square of sRGB `colour`, its top at y = 0.
    fn ground(&self, colour: [u8; 3]) -> Scene {
        let mut scene = Scene::default();
        let slab = Material::dielectric(colour.map(|c| f32::from(c) / 255.0), 0.92);
        scene.add(
            self.meshes.cube,
            Instance {
                frame: Frame64::from_position(DVec3::new(0.0, -0.1, 0.0)),
                scale: Vec3::new(80.0, 0.2, 80.0),
                material: slab,
                heat: None,
            },
        );
        scene
    }

    /// The board's scene on ground of sRGB `colour`.
    fn scene(&self, colour: [u8; 3]) -> Result<Scene, String> {
        let crate_model = (self.meshes.models.crate_box).ok_or("the packs have no crate model")?;
        let mut scene = self.ground(colour);
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
        // The smooth primitives (ADR-0026 decision 11): a bare steel member 2 m long lying
        // towards the camera, and a sphere in `steel_painted` (`art-style.md` §3).
        let srgb = |hex: u32| [hex >> 16, (hex >> 8) & 0xff, hex & 0xff].map(|c| c as f32 / 255.0);
        scene.add(
            self.meshes.cylinder,
            Instance {
                frame: Frame64::new(
                    DVec3::new(-3.2, 0.15, -1.0),
                    DQuat::from_rotation_x(std::f64::consts::FRAC_PI_2),
                ),
                scale: Vec3::new(0.3, 2.0, 0.3),
                material: Material::metal(srgb(0x8a_8d_90), 0.45),
                heat: None,
            },
        );
        scene.add(
            self.meshes.sphere,
            Instance {
                frame: Frame64::from_position(DVec3::new(3.2, 0.4, -0.7)),
                scale: Vec3::splat(0.8),
                material: Material::dielectric(srgb(0x6f_7a_6c), 0.6),
                heat: None,
            },
        );
        crate::style_people::add(&self.people, &mut scene);
        Ok(scene)
    }

    /// The board on `colour` with the sun at `sun` degrees, seen by `camera`.
    fn image(&mut self, scene: &Scene, sun: f64, camera: &Camera) -> Image {
        let sky = demo::sky(DVec3::ZERO);
        let lighting = sky.lighting(
            camera.position,
            demo::sun_direction(sun),
            &Lighting::default(),
        );
        self.renderer.capture(Frame {
            camera,
            scene,
            terrain: &[],
            terrain_frames: &[],
            lighting: &lighting,
            lines: &DebugLines::default(),
            sky: Some(&sky),
            heatmap: None,
            overlay: None,
            clear_color: creeer_render::wgpu::Color::BLACK,
        })
    }

    /// The engineer with each tool in hand (G1.4c), on `scene`'s slab, the sun at `sun`.
    fn tools(&mut self, scene: &Scene, sun: f64) -> Image {
        let sky = demo::sky(DVec3::ZERO);
        let at = DVec3::new(0.0, 1.0, 0.0);
        let lighting = sky.lighting(at, demo::sun_direction(sun), &Lighting::default());
        crate::style_people::tools_sheet(
            &mut self.renderer,
            &mut self.people,
            (&sky, &lighting),
            scene,
        )
    }

    /// The right hand close up, relaxed and gripping (G1.4b), on `scene`'s slab, the sun at
    /// `sun`.
    fn hands(&mut self, scene: &Scene, sun: f64) -> Image {
        let sky = demo::sky(DVec3::ZERO);
        let at = DVec3::new(0.0, 1.0, 0.0);
        let lighting = sky.lighting(at, demo::sun_direction(sun), &Lighting::default());
        crate::style_people::hands_sheet(&mut self.renderer, &self.people, (&sky, &lighting), scene)
    }

    /// A stride of the engineer at `speed` (m/s), on `scene`'s slab, the sun at `sun`.
    fn walk(&mut self, scene: &Scene, sun: f64, speed: f64) -> Image {
        let sky = demo::sky(DVec3::ZERO);
        let at = DVec3::new(0.0, 1.0, 0.0);
        let lighting = sky.lighting(at, demo::sun_direction(sun), &Lighting::default());
        crate::style_people::walk_sheet(
            &mut self.renderer,
            &self.people,
            (&sky, &lighting),
            scene,
            speed,
        )
    }
}

/// The board's own view, close; and from 20 m, as the third-person camera sees things far off
/// (`art-style.md` §7).
fn board_camera() -> Camera {
    let mut camera = Camera::new(DVec3::new(0.0, 1.9, 5.0));
    camera.pitch = -0.27;
    camera.fov_y = 45f32.to_radians();
    camera
}

/// The skin tones' view: the row of ten from in front.
fn tones_camera() -> Camera {
    let mut camera = Camera::new(DVec3::new(0.0, 1.5, 2.9));
    camera.pitch = -0.04;
    camera.fov_y = 45f32.to_radians();
    camera
}

fn far_camera() -> Camera {
    let mut camera = Camera::new(DVec3::new(0.0, 4.0, 20.0));
    camera.pitch = -0.19;
    camera
}

/// Renders the board described by `args` (after `--style-board`).
pub fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let value_of = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
    };
    let path = value_of("--style-board").ok_or("--style-board needs a file name")?;
    if args.iter().any(|a| a == "--all") {
        return all(Path::new(path), value_of("--check"), value_of("--save"));
    }
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
    let mut board = Board::new()?;
    let scene = board.scene(colour)?;
    // The board's view, or `--camera x y z yaw pitch` (m, degrees) for a close look.
    let mut camera = board_camera();
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
    board
        .image(&scene, sun, &camera)
        .save_png(Path::new(path))?;
    if let Some(sheet) = value_of("--walk") {
        board.walk(&scene, sun, 1.4).save_png(Path::new(sheet))?;
        println!("saved {sheet}: a stride walking at 1.4 m/s in eight frames");
    }
    if let Some(sheet) = value_of("--holding") {
        let sky = demo::sky(DVec3::ZERO);
        let at = DVec3::new(0.0, 1.0, 0.0);
        let lighting = sky.lighting(at, demo::sun_direction(sun), &Lighting::default());
        let b = &mut board;
        crate::style_holding::holding_sheet(
            &mut b.renderer,
            &mut b.people,
            &b.meshes,
            (&sky, &lighting),
            &scene,
        )
        .save_png(Path::new(sheet))?;
        println!("saved {sheet}: holding a box, a pole, a stick, and a box on the ground (G2.10)");
    }
    println!("saved {path}: the crate at {AGES:?} years on {ground}, sun {sun}°");
    Ok(())
}

/// Every image of the board into `dir`; checked against the golden images in `check`, or saved
/// as them in `save`.
fn all(
    dir: &Path,
    check: Option<&String>,
    save: Option<&String>,
) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(dir)?;
    let mut board = Board::new()?;
    let mut images: Vec<(String, Image)> = Vec::new();
    for (ground, colour) in GROUNDS {
        let scene = board.scene(colour)?;
        for (time, sun) in TIMES {
            images.push((
                format!("{ground}-{time}"),
                board.image(&scene, sun, &board_camera()),
            ));
        }
        if ground == "soil" {
            for (time, sun) in TIMES {
                images.push((
                    format!("{ground}-{time}-far"),
                    board.image(&scene, sun, &far_camera()),
                ));
            }
            images.push(("walk".into(), board.walk(&scene, TIMES[0].1, 1.4)));
            images.push(("run".into(), board.walk(&scene, TIMES[0].1, 3.5)));
            images.push(("hands".into(), board.hands(&scene, TIMES[0].1)));
            images.push(("tools".into(), board.tools(&scene, TIMES[0].1)));
            // The ten skin tones side by side (G1.3i).
            let mut tones = board.ground(colour);
            crate::style_people::add_tones(&board.people, &mut tones);
            images.push((
                "skin-tones".into(),
                board.image(&tones, TIMES[0].1, &tones_camera()),
            ));
        }
    }
    for (name, icon) in crate::icons::items(128)? {
        images.push((format!("icon-{name}"), icon));
    }
    let mut failed = Vec::new();
    for (name, image) in &images {
        let file = format!("{name}.png");
        image.save_png(&dir.join(&file))?;
        // Golden images are kept at half size (`golden::halved`).
        let half = crate::golden::halved(image);
        if let Some(golden) = save {
            std::fs::create_dir_all(golden)?;
            half.save_png(&PathBuf::from(golden).join(&file))?;
        }
        if let Some(golden) = check {
            let path = PathBuf::from(golden).join(&file);
            let Ok(want) = Image::load_png(&path) else {
                println!("{name}: no golden image ({})", path.display());
                failed.push(format!("{name} (no golden image)"));
                continue;
            };
            match crate::golden::compare(&half, &want) {
                Some(d) => {
                    let verdict = if d.passes() { "ok" } else { "CHANGED" };
                    println!(
                        "{name}: mean ΔE₀₀ {:.2}, {:.2} % of pixels noticeably different: {verdict}",
                        d.mean,
                        100.0 * d.noticed
                    );
                    if !d.passes() {
                        failed.push(name.clone());
                    }
                }
                None => failed.push(format!("{name} (another size)")),
            }
        }
    }
    println!("{} images in {}", images.len(), dir.display());
    if !failed.is_empty() {
        return Err(format!("changed from the golden images: {}", failed.join(", ")).into());
    }
    Ok(())
}
