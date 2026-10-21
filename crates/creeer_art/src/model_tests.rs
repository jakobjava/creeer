//! Models: the base pack's against the style and their manifest, and models broken in each way
//! the checks must catch, written here as binary glTF.

use crate::model::{Class, Entry, Model};
use crate::model_check::check;
use crate::palette::Palette;
use creeer_data::Data;
use std::f32::consts::TAU;

/// One primitive: its material, positions, normals and triangles.
type Primitive = (&'static str, Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<u32>);

/// A binary glTF of one node with one mesh of `primitives`; `extra` adds top-level JSON
/// members (`"images": [...]`).
fn glb(primitives: &[Primitive], extra: &str) -> Vec<u8> {
    let mut bin: Vec<u8> = Vec::new();
    let (mut views, mut accessors, mut prims, mut materials) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut view = |bin: &mut Vec<u8>, bytes: Vec<u8>| {
        let at = bin.len();
        bin.extend_from_slice(&bytes);
        while !bin.len().is_multiple_of(4) {
            bin.push(0);
        }
        views.push(format!(
            r#"{{"buffer":0,"byteOffset":{at},"byteLength":{}}}"#,
            bytes.len()
        ));
        views.len() - 1
    };
    for (k, (material, positions, normals, indices)) in primitives.iter().enumerate() {
        let floats = |v: &[[f32; 3]]| v.iter().flatten().flat_map(|x| x.to_le_bytes()).collect();
        let p = view(&mut bin, floats(positions));
        let n = view(&mut bin, floats(normals));
        let i = view(
            &mut bin,
            indices.iter().flat_map(|x| x.to_le_bytes()).collect(),
        );
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for q in positions {
            for a in 0..3 {
                lo[a] = lo[a].min(q[a]);
                hi[a] = hi[a].max(q[a]);
            }
        }
        let count = positions.len();
        accessors.push(format!(r#"{{"bufferView":{p},"componentType":5126,"count":{count},"type":"VEC3","min":{lo:?},"max":{hi:?}}}"#));
        accessors.push(format!(
            r#"{{"bufferView":{n},"componentType":5126,"count":{count},"type":"VEC3"}}"#
        ));
        accessors.push(format!(
            r#"{{"bufferView":{i},"componentType":5125,"count":{},"type":"SCALAR"}}"#,
            indices.len()
        ));
        let a = 3 * k;
        prims.push(format!(
            r#"{{"attributes":{{"POSITION":{a},"NORMAL":{}}},"indices":{},"material":{k}}}"#,
            a + 1,
            a + 2
        ));
        materials.push(format!(r#"{{"name":"{material}"}}"#));
    }
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],"meshes":[{{"primitives":[{}]}}],"materials":[{}],"accessors":[{}],"bufferViews":[{}],"buffers":[{{"byteLength":{}}}]{extra}}}"#,
        prims.join(","),
        materials.join(","),
        accessors.join(","),
        views.join(","),
        bin.len()
    );
    let mut json = json.into_bytes();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let total = 12 + 8 + json.len() + 8 + bin.len();
    let mut out = Vec::new();
    let word = |x: usize| u32::try_from(x).expect("small").to_le_bytes();
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2_u32.to_le_bytes());
    out.extend_from_slice(&word(total));
    out.extend_from_slice(&word(json.len()));
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&json);
    out.extend_from_slice(&word(bin.len()));
    out.extend_from_slice(b"BIN\0");
    out.extend_from_slice(&bin);
    out
}

/// A smooth-shaded open cylinder about +Y: `segments` around, radius `r`, height `h`.
fn cylinder(material: &'static str, segments: u32, r: f32, h: f32) -> Primitive {
    let (mut p, mut n, mut i) = (Vec::new(), Vec::new(), Vec::new());
    for s in 0..segments {
        #[allow(clippy::cast_precision_loss)] // a few segments
        let a = TAU * s as f32 / segments as f32;
        let (sin, cos) = a.sin_cos();
        for y in [0.0, h] {
            p.push([r * cos, y, -r * sin]);
            n.push([cos, 0.0, -sin]);
        }
        let (a0, a1, b0, b1) = (
            2 * s,
            2 * s + 1,
            (2 * s + 2) % (2 * segments),
            (2 * s + 3) % (2 * segments),
        );
        i.extend_from_slice(&[a0, b0, b1, a0, b1, a1]);
    }
    (material, p, n, i)
}

/// A unit square in the y = 0 plane, facing up.
fn square(material: &'static str) -> Primitive {
    let p = vec![
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 1.0],
        [1.0, 0.0, 0.0],
    ];
    (
        material,
        p,
        vec![[0.0, 1.0, 0.0]; 4],
        vec![0, 1, 2, 0, 2, 3],
    )
}

fn entry(class: Class, lo: [f64; 3], hi: [f64; 3]) -> Entry {
    Entry {
        name: "test".into(),
        file: "models/test.glb".into(),
        class,
        bounds: (lo, hi),
    }
}

fn palette() -> Palette {
    Palette::from_data(&Data::base()).expect("the base palette")
}

#[test]
fn the_base_packs_models_read_and_keep_the_style() {
    let data = Data::base();
    let palette = palette();
    let entries = Entry::all(&data).expect("the manifest");
    assert!(entries.iter().any(|e| e.name == "crate"));
    for e in &entries {
        let model = Model::read(data.binary(&e.file).expect("its file"), &palette)
            .unwrap_or_else(|why| panic!("{}: {why}", e.name));
        let problems = check(&model, e);
        assert!(problems.is_empty(), "{}: {problems:#?}", e.name);
    }
}

#[test]
fn the_crates_bake_is_there_and_sensible() {
    let data = Data::base();
    let model = Model::read(
        data.binary("models/crate.glb").expect("the crate"),
        &palette(),
    )
    .expect("reads");
    assert_eq!(model.triangles(), 788);
    let palette = palette();
    let tokens: std::collections::BTreeSet<u16> = model.vertices.iter().map(|v| v.token).collect();
    let named: Vec<&str> = tokens
        .iter()
        .map(|&t| palette.tokens[t as usize].name.as_str())
        .collect();
    assert_eq!(named, ["steel_painted", "timber"]);
    // Open faces see the whole sky; the gaps between boards see little of it.
    let occlusion: Vec<f32> = model.vertices.iter().map(|v| v.occlusion).collect();
    assert!(occlusion.iter().any(|&o| o > 0.999));
    assert!(occlusion.iter().any(|&o| o < 0.3));
    // Wear lies on the bevels: some of the vertices, never all.
    let worn = model.vertices.iter().filter(|v| v.wear > 0.5).count();
    let share = worn as f64 / model.vertices.len() as f64;
    assert!((0.05..0.6).contains(&share), "{share}");
}

#[test]
fn broken_models_are_refused_for_their_reason() {
    let palette = palette();
    let read = |bytes: Vec<u8>| Model::read(&bytes, &palette);
    let refused = |bytes: Vec<u8>, why: &str| {
        let error = read(bytes).expect_err(why);
        assert!(error.contains(why), "{error:?} should say {why:?}");
    };
    refused(glb(&[square("unobtainium")], ""), "not a palette token");
    refused(
        glb(&[square("steel")], r#","images":[{"uri":"photo.png"}]"#),
        "images",
    );
    refused(
        glb(&[square("steel")], r#","skins":[{"joints":[0]}]"#),
        "skin",
    );
    refused(b"not a model".to_vec(), "not a glTF");
    assert!(read(glb(&[square("steel")], "")).is_ok());
}

#[test]
fn checks_find_budgets_bounds_and_bad_geometry() {
    let palette = palette();
    let read = |p: Vec<Primitive>| Model::read(&glb(&p, ""), &palette).expect("reads");
    let says = |problems: Vec<String>, what: &str| {
        assert!(
            problems.iter().any(|p| p.contains(what)),
            "{what:?} not in {problems:#?}"
        );
    };
    // A smooth cylinder of 64 segments passes; one of 6 is faceted.
    let fine = read(vec![cylinder("steel", 64, 0.5, 1.0)]);
    let item = entry(Class::Item, [-0.5, 0.0, -0.5], [0.5, 1.0, 0.5]);
    assert!(check(&fine, &item).is_empty(), "{:#?}", check(&fine, &item));
    says(
        check(&read(vec![cylinder("steel", 6, 0.5, 1.0)]), &item),
        "strays",
    );
    // 128 triangles: over a decorative's 200 only when doubled.
    says(
        check(
            &read(vec![
                cylinder("steel", 64, 0.5, 1.0),
                cylinder("timber", 64, 0.5, 1.0),
            ]),
            &entry(Class::Decorative, [-0.5, 0.0, -0.5], [0.5, 1.0, 0.5]),
        ),
        "triangles",
    );
    // A metre too tall for its manifest, or its origin elsewhere.
    says(
        check(
            &fine,
            &entry(Class::Item, [-0.5, 0.0, -0.5], [0.5, 2.0, 0.5]),
        ),
        "most y",
    );
    says(
        check(
            &fine,
            &entry(Class::Item, [-0.5, -0.5, -0.5], [0.5, 0.5, 0.5]),
        ),
        "least y",
    );
    // A triangle with no area, and a normal that is not a unit vector.
    let (m, mut p, mut n, mut i) = square("steel");
    p.push([2.0, 0.0, 0.0]);
    n.push([0.0, 0.0, 0.0]);
    i.extend_from_slice(&[0, 3, 4]);
    let bad = read(vec![(m, p, n, i)]);
    let unit = entry(Class::Item, [0.0, 0.0, 0.0], [2.0, 0.0, 1.0]);
    says(check(&bad, &unit), "degenerate");
    says(check(&bad, &unit), "unit vector");
}

#[test]
fn the_exported_masks_are_the_baked_values_unconverted() {
    // art/bake.py stores each corner's height above the base over 2 m in alpha: linear in the
    // position, so every vertex's alpha must equal its y / 2 to the attribute's 16 bits. An
    // export that treated the attribute as colour (sRGB) would bend it. Checked by hand once
    // too: Blender's occlusion 0.6875 and 0.6458 at two corners read back exactly (2026-10-21).
    // The grain coordinates come through as Blender wrote them, v unflipped.
    let data = Data::base();
    let model = Model::read(
        data.binary("models/crate.glb").expect("the crate"),
        &palette(),
    )
    .expect("reads");
    let (lo, _) = model.bounds();
    for v in &model.vertices {
        let height = (v.position[1] - lo.y) / crate::model::HEIGHT_RANGE;
        assert!(
            (v.height - height).abs() < 1e-4,
            "height {} at {:?}",
            v.height,
            v.position
        );
    }
    // On the boards' outer faces of one side (art/crate.py: 0.482 m out, running along x), u
    // runs along the board in metres: its gradient in the face is (1, 0).
    let mut boards = 0;
    for t in model.indices.as_chunks::<3>().0 {
        let [a, b, c] = [t[0], t[1], t[2]].map(|i| model.vertices[i as usize]);
        let on_board = [a, b, c]
            .iter()
            .all(|v| v.normal[2] > 0.99 && (v.position[2] - 0.482).abs() < 5e-4);
        if !on_board {
            continue;
        }
        let (e1, e2) = (
            [b.position[0] - a.position[0], b.position[1] - a.position[1]],
            [c.position[0] - a.position[0], c.position[1] - a.position[1]],
        );
        let (d1, d2) = (b.grain[0] - a.grain[0], c.grain[0] - a.grain[0]);
        let det = e1[0] * e2[1] - e1[1] * e2[0];
        let gradient = [
            (d1 * e2[1] - d2 * e1[1]) / det,
            (e1[0] * d2 - e2[0] * d1) / det,
        ];
        assert!(
            (gradient[0] - 1.0).abs() < 1e-3 && gradient[1].abs() < 1e-3,
            "{gradient:?}"
        );
        boards += 1;
    }
    assert!(boards >= 8, "only {boards} board triangles found");
}
