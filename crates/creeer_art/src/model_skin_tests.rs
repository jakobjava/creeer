//! Skinned models (G1.3f): files written here as binary glTF with a skin of named joints and
//! meshes named as parts, read back, held to their checks, and broken in each way the reader
//! must refuse.

use crate::model::{Class, Entry, Model};
use crate::model_check::check;
use crate::palette::Palette;
use creeer_data::Data;

/// One part: its node's name, material, positions, normals, triangles, and each vertex's
/// joints and weights.
struct Part {
    name: &'static str,
    material: &'static str,
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    indices: Vec<u32>,
    joints: Vec<[u8; 4]>,
    weights: Vec<[f32; 4]>,
}

/// A binary glTF of `bones` (named joints placed at their heads, at rest unless `unbind`
/// moves an inverse bind matrix) and `parts` on one skin.
fn skinned(bones: &[(&str, [f32; 3])], parts: &[Part], unbind: [f32; 3]) -> Vec<u8> {
    let mut bin: Vec<u8> = Vec::new();
    let (mut views, mut accessors) = (Vec::new(), Vec::new());
    let mut add = |bin: &mut Vec<u8>, bytes: Vec<u8>, accessor: String| {
        let at = bin.len();
        bin.extend_from_slice(&bytes);
        while !bin.len().is_multiple_of(4) {
            bin.push(0);
        }
        views.push(format!(
            r#"{{"buffer":0,"byteOffset":{at},"byteLength":{}}}"#,
            bytes.len()
        ));
        accessors.push(accessor.replace("VIEW", &(views.len() - 1).to_string()));
        accessors.len() - 1
    };
    let floats = |v: &[f32]| v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
    let mut matrices = Vec::new();
    for (k, (_, head)) in bones.iter().enumerate() {
        let shift = if k == 0 { unbind } else { [0.0; 3] };
        let m = [
            1.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
            -head[0] + shift[0],
            -head[1] + shift[1],
            -head[2] + shift[2],
            1.0,
        ];
        matrices.extend_from_slice(&m);
    }
    let unbinds = add(
        &mut bin,
        floats(&matrices),
        format!(
            r#"{{"bufferView":VIEW,"componentType":5126,"count":{},"type":"MAT4"}}"#,
            bones.len()
        ),
    );
    let (mut meshes, mut materials, mut nodes) = (Vec::new(), Vec::new(), Vec::new());
    for (name, head) in bones {
        nodes.push(format!(r#"{{"name":"{name}","translation":{head:?}}}"#));
    }
    for (k, part) in parts.iter().enumerate() {
        let count = part.positions.len();
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for q in &part.positions {
            for a in 0..3 {
                lo[a] = lo[a].min(q[a]);
                hi[a] = hi[a].max(q[a]);
            }
        }
        let p = add(
            &mut bin,
            floats(part.positions.as_flattened()),
            format!(
                r#"{{"bufferView":VIEW,"componentType":5126,"count":{count},"type":"VEC3","min":{lo:?},"max":{hi:?}}}"#
            ),
        );
        let n = add(
            &mut bin,
            floats(part.normals.as_flattened()),
            format!(r#"{{"bufferView":VIEW,"componentType":5126,"count":{count},"type":"VEC3"}}"#),
        );
        let j = add(
            &mut bin,
            part.joints.as_flattened().to_vec(),
            format!(r#"{{"bufferView":VIEW,"componentType":5121,"count":{count},"type":"VEC4"}}"#),
        );
        let w = add(
            &mut bin,
            floats(part.weights.as_flattened()),
            format!(r#"{{"bufferView":VIEW,"componentType":5126,"count":{count},"type":"VEC4"}}"#),
        );
        let i = add(
            &mut bin,
            part.indices.iter().flat_map(|x| x.to_le_bytes()).collect(),
            format!(
                r#"{{"bufferView":VIEW,"componentType":5125,"count":{},"type":"SCALAR"}}"#,
                part.indices.len()
            ),
        );
        meshes.push(format!(r#"{{"primitives":[{{"attributes":{{"POSITION":{p},"NORMAL":{n},"JOINTS_0":{j},"WEIGHTS_0":{w}}},"indices":{i},"material":{k}}}]}}"#));
        materials.push(format!(r#"{{"name":"{}"}}"#, part.material));
        nodes.push(format!(r#"{{"name":"{}","mesh":{k},"skin":0}}"#, part.name));
    }
    let joints: Vec<String> = (0..bones.len()).map(|k| k.to_string()).collect();
    let roots: Vec<String> = (0..nodes.len()).map(|k| k.to_string()).collect();
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[{}]}}],"nodes":[{}],"skins":[{{"joints":[{}],"inverseBindMatrices":{unbinds}}}],"meshes":[{}],"materials":[{}],"accessors":[{}],"bufferViews":[{}],"buffers":[{{"byteLength":{}}}]}}"#,
        roots.join(","),
        nodes.join(","),
        joints.join(","),
        meshes.join(","),
        materials.join(","),
        accessors.join(","),
        views.join(","),
        bin.len()
    );
    let mut json = json.into_bytes();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let word = |x: usize| u32::try_from(x).expect("small").to_le_bytes();
    let mut out = Vec::new();
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2_u32.to_le_bytes());
    out.extend_from_slice(&word(12 + 8 + json.len() + 8 + bin.len()));
    out.extend_from_slice(&word(json.len()));
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&json);
    out.extend_from_slice(&word(bin.len()));
    out.extend_from_slice(b"BIN\0");
    out.extend_from_slice(&bin);
    out
}

/// A smooth open cylinder about +Y as a part: `segments` round, radius `r`, from `y0` to
/// `y1`, its bottom ring on bone 0 and its top ring on bone 1.
fn tube(name: &'static str, segments: u32, r: f32, y0: f32, y1: f32) -> Part {
    let (mut positions, mut normals, mut indices, mut joints, mut weights) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for s in 0..segments {
        #[allow(clippy::cast_precision_loss)] // a few segments
        let a = std::f32::consts::TAU * s as f32 / segments as f32;
        let (sin, cos) = a.sin_cos();
        for (y, bone) in [(y0, 0), (y1, 1)] {
            positions.push([r * cos, y, -r * sin]);
            normals.push([cos, 0.0, -sin]);
            joints.push([bone, 0, 0, 0]);
            weights.push([1.0, 0.0, 0.0, 0.0]);
        }
        let (a0, a1, b0, b1) = (
            2 * s,
            2 * s + 1,
            (2 * s + 2) % (2 * segments),
            (2 * s + 3) % (2 * segments),
        );
        indices.extend_from_slice(&[a0, b0, b1, a0, b1, a1]);
    }
    Part {
        name,
        material: "coverall",
        positions,
        normals,
        indices,
        joints,
        weights,
    }
}

const BONES: [(&str, [f32; 3]); 2] = [("thigh.L", [0.1, 0.96, 0.0]), ("shank.L", [0.1, 0.52, 0.0])];

fn palette() -> Palette {
    Palette::from_data(&Data::base()).expect("the base palette")
}

fn read(bytes: &[u8]) -> Result<Model, String> {
    Model::read(bytes, &palette())
}

#[test]
fn a_skinned_model_reads_its_bones_parts_and_weights() {
    let model = read(&skinned(
        &BONES,
        &[
            tube("coverall:thigh.L@0", 64, 0.08, 0.5, 0.96),
            tube("coverall:thigh.L@2", 8, 0.08, 0.5, 0.96),
        ],
        [0.0; 3],
    ))
    .expect("reads");
    let names: Vec<&str> = model.bones.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(names, ["thigh.L", "shank.L"]);
    assert_eq!(model.bones[1].head, [0.1, 0.52, 0.0]);
    assert_eq!(model.parts.len(), 2);
    let (close, far) = (&model.parts[0], &model.parts[1]);
    assert_eq!(
        (close.piece.as_str(), close.region.as_str(), close.level),
        ("coverall", "thigh.L", 0)
    );
    assert_eq!(
        (far.level, far.triangles(), far.first),
        (2, 16, close.count)
    );
    // The bottom ring on the first bone, the top on the second, wholly.
    assert!(model.vertices.iter().all(
        |v| (v.weights[0] - 1.0).abs() < 1e-6 && v.weights[1..].iter().all(|w| w.abs() < 1e-6)
    ));
    assert!(
        model.vertices.iter().any(|v| v.bones[0] == 1)
            && model.vertices.iter().any(|v| v.bones[0] == 0)
    );
}

#[test]
fn a_skinned_model_is_refused_for_its_reason() {
    let refused = |bytes: Vec<u8>, why: &str| {
        let error = read(&bytes).expect_err(why);
        assert!(error.contains(why), "{error:?} should say {why:?}");
    };
    refused(
        skinned(
            &BONES,
            &[tube("coverall-thigh", 8, 0.08, 0.5, 0.96)],
            [0.0; 3],
        ),
        "not <piece>:<region>@<level>",
    );
    refused(
        skinned(
            &BONES,
            &[tube("coverall:tail@0", 8, 0.08, 0.5, 0.96)],
            [0.0; 3],
        ),
        "no region",
    );
    refused(
        skinned(
            &BONES,
            &[tube("coverall:thigh.L@3", 8, 0.08, 0.5, 0.96)],
            [0.0; 3],
        ),
        "past the last",
    );
    refused(
        skinned(
            &BONES,
            &[tube("coverall:thigh.L@0", 8, 0.08, 0.5, 0.96)],
            [0.0, 0.01, 0.0],
        ),
        "not at rest",
    );
    let mut stray = tube("coverall:thigh.L@0", 8, 0.08, 0.5, 0.96);
    stray.joints[0] = [2, 0, 0, 0];
    refused(skinned(&BONES, &[stray], [0.0; 3]), "a bone the skin lacks");
}

#[test]
fn a_skinned_models_checks_hold_each_level_and_its_weights() {
    let entry = Entry {
        name: "test".into(),
        file: "models/test.glb".into(),
        class: Class::Character,
        bounds: ([-0.08, 0.5, -0.08], [0.08, 0.96, 0.08]),
    };
    let says = |problems: &[String], what: &str| {
        assert!(
            problems.iter().any(|p| p.contains(what)),
            "{what:?} not in {problems:#?}"
        )
    };
    // Smooth close up, coarse far off: only the close-up level's facets are held to §7.
    let fine = read(&skinned(
        &BONES,
        &[
            tube("coverall:thigh.L@0", 64, 0.08, 0.5, 0.96),
            tube("coverall:thigh.L@2", 8, 0.08, 0.5, 0.96),
        ],
        [0.0; 3],
    ))
    .expect("reads");
    assert!(
        check(&fine, &entry).is_empty(),
        "{:#?}",
        check(&fine, &entry)
    );
    let coarse = read(&skinned(
        &BONES,
        &[tube("coverall:thigh.L@0", 8, 0.08, 0.5, 0.96)],
        [0.0; 3],
    ))
    .expect("reads");
    says(&check(&coarse, &entry), "strays");
    // A wearable's far level holds 800: 1,000 triangles are over, at that level only.
    let heavy = read(&skinned(
        &BONES,
        &[tube("coverall:thigh.L@2", 500, 0.08, 0.5, 0.96)],
        [0.0; 3],
    ))
    .expect("reads");
    let problems = check(&heavy, &entry);
    says(
        &problems,
        "coverall at level 2: 1000 triangles, over a Wearable's 800",
    );
    // Weights that do not sum to one.
    let mut light = tube("coverall:thigh.L@0", 64, 0.08, 0.5, 0.96);
    light.weights[3] = [0.5, 0.0, 0.0, 0.0];
    says(
        &check(
            &read(&skinned(&BONES, &[light], [0.0; 3])).expect("reads"),
            &entry,
        ),
        "do not sum to one",
    );
}
