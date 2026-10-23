//! Skinning on a real GPU (G1.3e, ADR-0026 decision 6): the engineer skinned by `skin.wgsl`,
//! read back and held to the CPU's dual quaternions (`creeer_render::skin`), which their own
//! tests hold to rigid motions. Skipped, with a message, without a GPU.

use creeer_art::Palette;
use creeer_art::model::{Entry, Model};
use creeer_art::wearable::{Wearable, visible};
use creeer_data::Data;
use creeer_math::Frame64;
use creeer_math::det;
use creeer_math::glam::{DQuat, DVec3, Vec3};
use creeer_render::renderer::SkinnedVertex;
use creeer_render::skin::{DualQuat, SkinnedMeshData, pack_weights};
use creeer_render::{
    Camera, Character, DebugLines, Frame, Lighting, Material, Renderer, Scene, wgpu,
};

struct Engineer {
    palette: Palette,
    model: Model,
    data: SkinnedMeshData,
    wearables: Vec<Wearable>,
}

fn engineer() -> Engineer {
    let data = Data::base();
    let palette = Palette::from_data(&data).expect("the palette");
    let entry = (Entry::all(&data).expect("the manifest").into_iter())
        .find(|e| e.name == "engineer")
        .expect("the engineer");
    let model =
        Model::read(data.binary(&entry.file).expect("its file"), &palette).expect("it reads");
    let skinned = SkinnedMeshData::new(&model, entry.levels.expect("its levels"));
    Engineer {
        palette,
        model,
        data: skinned,
        wearables: Wearable::all(&data).expect("the wearables"),
    }
}

/// Draws the engineer, wearing everything, posed by `bones`, from a camera at the origin
/// looking at him (so camera-relative is world), and reads back his close-up level.
fn skinned(e: &Engineer, bones: Vec<Frame64>) -> Option<Vec<SkinnedVertex>> {
    let mut renderer = match pollster::block_on(Renderer::new_headless((320, 180))) {
        Ok(renderer) => renderer,
        Err(error) => {
            eprintln!("skipping GPU test: {error}");
            return None;
        }
    };
    renderer.set_palette(&e.palette);
    let id = renderer.add_skinned(&e.data);
    let all: Vec<&str> = e.wearables.iter().map(|w| w.name.as_str()).collect();
    let outfit = renderer.add_outfit(id, &visible(&e.model, &e.wearables, &all));
    let mut camera = Camera::new(DVec3::ZERO);
    camera.yaw = std::f64::consts::PI;
    let mut scene = Scene::default();
    scene.characters.push(Character {
        outfit,
        bones,
        colours: [[0.5, 0.1, 0.1], [0.6, 0.4, 0.3], [0.1, 0.08, 0.06]],
        material: Material::dielectric([1.0; 3], 0.8),
    });
    renderer.capture(Frame {
        camera: &camera,
        scene: &scene,
        terrain: &[],
        terrain_frames: &[],
        lighting: &Lighting::default(),
        lines: &DebugLines::default(),
        sky: None,
        heatmap: None,
        overlay: None,
        clear_color: wgpu::Color::BLACK,
    });
    let level = &e.data.levels[0];
    Some(renderer.read_skinned((level.end - level.start) as usize))
}

/// Him standing 1 m ahead of the camera (+z), facing it.
fn placed() -> Frame64 {
    Frame64::new(
        DVec3::new(0.0, -1.0, 1.5),
        DQuat::from_rotation_y(std::f64::consts::PI),
    )
}

#[test]
fn the_rest_pose_draws_the_model_itself() {
    let e = engineer();
    let Some(out) = skinned(&e, vec![placed(); e.data.bones]) else {
        return;
    };
    let start = e.data.levels[0].start as usize;
    let place = placed();
    let players: Vec<usize> = ["player_accent", "skin", "hair"]
        .iter()
        .map(|n| e.palette.index(n).expect("a per-player token"))
        .collect();
    let slot = u32::try_from(e.palette.tokens.len()).expect("few tokens");
    let mut moved = 0.0f32;
    for (k, v) in out.iter().enumerate() {
        let rest = &e.data.model.vertices[start + k];
        let expected = place
            .transform_point(Vec3::from(rest.position).as_dvec3())
            .as_vec3();
        moved = moved.max((Vec3::from(v.position) - expected).length());
        let n =
            (place.rotation.as_quat() * Vec3::from(rest.normal) - Vec3::from(v.normal())).length();
        // Both packed in 16 bits.
        assert!(n < 1e-4, "vertex {k}'s normal off by {n}");
        // Per-player tokens point at the character's slots, the rest unchanged.
        let token = v.material & 0xffff;
        match players.iter().position(|&p| p == usize::from(rest.token)) {
            Some(s) => assert_eq!(token, slot + u32::try_from(s).expect("three"), "vertex {k}"),
            None => assert_eq!(token, u32::from(rest.token), "vertex {k}"),
        }
    }
    assert!(moved < 2e-6, "the rest pose moved a vertex by {moved} m");
}

/// Each bone turned and moved its own way: the GPU's blend is the CPU's, vertex for vertex.
#[test]
fn a_pose_skins_as_the_cpu_does() {
    let e = engineer();
    let bones: Vec<Frame64> = (0..e.data.bones)
        .map(|k| {
            #[allow(clippy::cast_precision_loss)]
            let a = 0.37 * k as f64;
            let (sin, cos) = det::sin_cos(a);
            let axis = DVec3::new(cos, 1.0, sin).normalize();
            Frame64::new(
                DVec3::new(0.05 * sin, -1.0 + 0.02 * a, 1.5),
                DQuat::from_axis_angle(axis, 0.4 + 0.3 * cos),
            )
        })
        .collect();
    let Some(out) = skinned(&e, bones.clone()) else {
        return;
    };
    let dqs: Vec<DualQuat> = bones
        .iter()
        .map(|b| DualQuat::from_rigid(b.rotation.as_quat(), b.position.as_vec3()))
        .collect();
    let start = e.data.levels[0].start as usize;
    let mut worst = 0.0f32;
    for (k, v) in out.iter().enumerate() {
        let rest = &e.data.model.vertices[start + k];
        // The weights as the GPU has them, in bytes.
        let weights = pack_weights(rest.weights);
        let blend: Vec<(DualQuat, f32)> = (0..4)
            .filter(|&i| weights[i] > 0)
            .map(|i| {
                (
                    dqs[usize::from(rest.bones[i])],
                    f32::from(weights[i]) / 255.0,
                )
            })
            .collect();
        let cpu = DualQuat::blend(&blend);
        worst = worst.max((cpu.point(Vec3::from(rest.position)) - Vec3::from(v.position)).length());
        let n = (cpu.direction(Vec3::from(rest.normal)) - Vec3::from(v.normal())).length();
        assert!(n < 1e-4, "vertex {k}'s normal off by {n}");
    }
    println!(
        "GPU against CPU skinning: {worst:e} m at worst over {} vertices",
        out.len()
    );
    assert!(worst < 5e-6, "{worst}");
}

/// Several characters in one dispatch, each its own job (G1.3e: a shader struct of the wrong
/// size once skinned only the first): the second's vertices are the CPU's too.
#[test]
fn every_character_of_a_frame_is_skinned() {
    let e = engineer();
    let mut renderer = match pollster::block_on(Renderer::new_headless((320, 180))) {
        Ok(renderer) => renderer,
        Err(error) => {
            eprintln!("skipping GPU test: {error}");
            return;
        }
    };
    renderer.set_palette(&e.palette);
    let id = renderer.add_skinned(&e.data);
    let all: Vec<&str> = e.wearables.iter().map(|w| w.name.as_str()).collect();
    let outfit = renderer.add_outfit(id, &visible(&e.model, &e.wearables, &all));
    let mut camera = Camera::new(DVec3::ZERO);
    camera.yaw = std::f64::consts::PI;
    let turned =
        |x: f64, turn: f64| Frame64::new(DVec3::new(x, -1.0, 2.0), DQuat::from_rotation_y(turn));
    let mut scene = Scene::default();
    for (k, x) in [-0.6, 0.0, 0.6].into_iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let bones = vec![turned(x, 0.5 * k as f64); e.data.bones];
        scene.characters.push(Character {
            outfit,
            bones,
            colours: [[0.5; 3]; 3],
            material: Material::dielectric([1.0; 3], 0.8),
        });
    }
    renderer.capture(Frame {
        camera: &camera,
        scene: &scene,
        terrain: &[],
        terrain_frames: &[],
        lighting: &Lighting::default(),
        lines: &DebugLines::default(),
        sky: None,
        heatmap: None,
        overlay: None,
        clear_color: wgpu::Color::BLACK,
    });
    let level = &e.data.levels[0];
    let count = (level.end - level.start) as usize;
    let out = renderer.read_skinned(3 * count);
    for (c, bone) in scene.characters.iter().map(|ch| ch.bones[0]).enumerate() {
        for k in (0..count).step_by(97) {
            let rest = &e.data.model.vertices[level.start as usize + k];
            let expected = bone
                .transform_point(Vec3::from(rest.position).as_dvec3())
                .as_vec3();
            let got = Vec3::from(out[c * count + k].position);
            assert!(
                (got - expected).length() < 2e-6,
                "character {c}, vertex {k}"
            );
        }
    }
}
