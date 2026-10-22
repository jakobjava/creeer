use super::{grouped, level_errors};
use crate::Palette;
use crate::model::{Entry, Model, Vertex};
use crate::model_skin::Part;
use creeer_data::Data;
use creeer_math::det;

fn vertex(position: [f32; 3]) -> Vertex {
    Vertex {
        position,
        normal: [0.0, 1.0, 0.0],
        token: 0,
        occlusion: 1.0,
        wear: 0.0,
        tone: 0.5,
        height: 0.0,
        grain: [0.0; 2],
        bones: [0; 4],
        weights: [1.0, 0.0, 0.0, 0.0],
    }
}

/// A regular `n`-gon of radius 1 in the plane y = 0, as a fan from its centre.
fn polygon(model: &mut Model, n: u32, level: u8) {
    let base = u32::try_from(model.vertices.len()).expect("small");
    let first = model.indices.len();
    model.vertices.push(vertex([0.0; 3]));
    for k in 0..n {
        let (s, c) = det::sin_cos(std::f64::consts::TAU * f64::from(k) / f64::from(n));
        #[allow(clippy::cast_possible_truncation)]
        model.vertices.push(vertex([c as f32, 0.0, s as f32]));
    }
    for k in 0..n {
        model
            .indices
            .extend([base, base + 1 + (k + 1) % n, base + 1 + k]);
    }
    model.parts.push(Part {
        piece: "disc".into(),
        region: "torso".into(),
        level,
        first,
        count: model.indices.len() - first,
    });
}

/// Three levels of a disc: 64, 16 and 8 sides, the levels' vertices interleaved in the file.
fn discs() -> Model {
    let mut model = Model::default();
    polygon(&mut model, 16, 1);
    polygon(&mut model, 64, 0);
    polygon(&mut model, 8, 2);
    model
}

#[test]
fn a_level_errs_by_its_sagitta() {
    // A coarse polygon's edge lies inside the fine one's rim by the sagitta 1 − cos(π/n), and
    // the fine one's rim stands out of the coarse polygon by about as much.
    let errors = level_errors(&discs());
    let sagitta = |n: f64| 1.0 - det::cos(std::f64::consts::PI / n);
    assert!(errors[0].abs() < f32::EPSILON);
    #[allow(clippy::cast_possible_truncation)]
    let (mid, far) = (sagitta(16.0) as f32, sagitta(8.0) as f32);
    assert!((errors[1] - mid).abs() < 0.01 * mid, "{errors:?}, {mid}");
    assert!((errors[2] - far).abs() < 0.01 * far, "{errors:?}, {far}");
}

#[test]
fn grouping_puts_each_level_together_and_keeps_every_triangle() {
    let model = discs();
    let (out, ranges) = grouped(&model);
    assert_eq!(ranges, [0..65, 65..82, 82..91]);
    assert_eq!(out.parts, model.parts);
    for part in &out.parts {
        let range = &ranges[usize::from(part.level)];
        for &i in &out.indices[part.first..part.first + part.count] {
            assert!(range.contains(&i), "level {} uses {i}", part.level);
        }
    }
    // The same triangles, corner for corner.
    let corners = |m: &Model| -> Vec<[f32; 3]> {
        m.indices
            .iter()
            .map(|&i| m.vertices[i as usize].position)
            .collect()
    };
    assert_eq!(corners(&out), corners(&model));
}

/// The engineer's levels against the close-up one, as the art pipeline measured them piece by
/// piece (ADR-0026, Validation: mid 0.9–7.4 mm, far 2.4–36 mm).
#[test]
fn the_engineer_levels_err_as_measured() {
    let data = Data::base();
    let palette = Palette::from_data(&data).expect("the palette");
    let entry = (Entry::all(&data).expect("the manifest").into_iter())
        .find(|e| e.name == "engineer")
        .expect("the engineer");
    let model =
        Model::read(data.binary(&entry.file).expect("its file"), &palette).expect("it reads");
    let errors = level_errors(&model);
    println!("the engineer's levels err by {errors:?} m");
    // The far level as the art pipeline found it (36 mm); the mid one above its 7.4 mm, as
    // this measures both ways (the eyelets and buttons it loses).
    assert!(errors[1] > 0.0074 && errors[1] < 0.015, "{errors:?}");
    assert!(errors[2] > 0.030 && errors[2] < 0.040, "{errors:?}");
    // The manifest keeps them, rounded up by under 1 %.
    let kept = entry.levels.expect("the manifest keeps its levels' errors");
    for (kept, measured) in kept.iter().zip(errors) {
        assert!(
            *kept >= measured && *kept <= measured * 1.01 + 1e-6,
            "{kept} for {measured}"
        );
    }
}
