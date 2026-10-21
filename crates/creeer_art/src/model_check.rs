//! A model held to the style and to its manifest entry (`docs/art-style.md` §7, §11.3).
//!
//! - its triangles within its class's budget;
//! - its bounding box as the manifest says, within a centimetre (size, scale and origin);
//! - unit normals and no degenerate triangles;
//! - smooth curves: across every smoothly shaded edge, the facets stray from the curve they
//!   stand for by at most 1 % of its radius or 1 mm, whichever is more.

use crate::model::{Entry, Model};
use creeer_math::det;
use creeer_math::glam::Vec3;
use std::collections::HashMap;

/// How far the bounding box may differ from the manifest's (m).
pub const BOUNDS_TOLERANCE: f64 = 0.01;
/// The largest share of a curve's radius its facets may stray from it.
pub const SAGITTA_SHARE: f64 = 0.01;
/// The facets may always stray this far (m): below it no one sees a facet.
pub const SAGITTA_FLOOR: f64 = 0.001;
/// The smallest triangle (m²).
pub const SMALLEST_AREA: f64 = 1e-10;

/// What is wrong with `model` as `entry` describes it; empty when nothing is.
pub fn check(model: &Model, entry: &Entry) -> Vec<String> {
    let mut problems = Vec::new();
    let budget = entry.class.budget();
    if model.triangles() > budget {
        problems.push(format!(
            "{} triangles, over a {:?}'s {budget}",
            model.triangles(),
            entry.class
        ));
    }
    let (lo, hi) = model.bounds();
    let (want_lo, want_hi) = entry.bounds;
    for axis in 0..3 {
        for (got, want, side) in [
            (lo[axis], want_lo[axis], "least"),
            (hi[axis], want_hi[axis], "most"),
        ] {
            if (f64::from(got) - want).abs() > BOUNDS_TOLERANCE {
                problems.push(format!(
                    "its {side} {} is {got:.3} m, the manifest says {want:.3} m",
                    ["x", "y", "z"][axis]
                ));
            }
        }
    }
    if let Some(v) = model
        .vertices
        .iter()
        .find(|v| (Vec3::from(v.normal).length() - 1.0).abs() > 1e-3)
    {
        problems.push(format!(
            "a normal that is not a unit vector at {:?}",
            v.position
        ));
    }
    let degenerate = (model.indices.as_chunks::<3>().0.iter())
        .filter(|t| f64::from(area(model, t.as_slice())) < SMALLEST_AREA)
        .count();
    if degenerate > 0 {
        problems.push(format!("{degenerate} degenerate triangles"));
    }
    if let Some((stray, limit, at)) = worst_facet(model) {
        problems.push(format!(
            "a smoothly shaded edge at {at:?} strays {:.1} mm from its curve, more than {:.1} mm: \
             it needs more segments (art-style.md §7)",
            stray * 1e3,
            limit * 1e3
        ));
    }
    problems
}

fn corner(model: &Model, i: u32) -> Vec3 {
    Vec3::from(model.vertices[i as usize].position)
}

fn area(model: &Model, t: &[u32]) -> f32 {
    let [a, b, c] = [t[0], t[1], t[2]].map(|i| corner(model, i));
    (b - a).cross(c - a).length() / 2.0
}

/// A vertex as shading sees it: where it is and which way it faces, to 0.1 mm and 0.001.
fn key(model: &Model, i: u32) -> [i32; 6] {
    let v = &model.vertices[i as usize];
    let q = |x: f32, step: f32| {
        #[allow(clippy::cast_possible_truncation)] // models are metres across
        let k = (x / step).round() as i32;
        k
    };
    [
        q(v.position[0], 1e-4),
        q(v.position[1], 1e-4),
        q(v.position[2], 1e-4),
        q(v.normal[0], 1e-3),
        q(v.normal[1], 1e-3),
        q(v.normal[2], 1e-3),
    ]
}

/// The smoothly shaded edge whose facets stray furthest beyond their limit: (stray, limit,
/// where), if any does.
fn worst_facet(model: &Model) -> Option<(f64, f64, [f32; 3])> {
    // Edges by their two ends' shading keys: two triangles that share both share a smooth
    // edge. Each with the triangle and the edge's two vertices.
    type Shared = Vec<(usize, u32, u32)>;
    let mut edges: HashMap<([i32; 6], [i32; 6]), Shared> = HashMap::new();
    for (k, t) in model.indices.as_chunks::<3>().0.iter().enumerate() {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let (ka, kb) = (key(model, a), key(model, b));
            let id = if ka < kb { (ka, kb) } else { (kb, ka) };
            edges.entry(id).or_default().push((k, a, b));
        }
    }
    let tri = |k: usize| &model.indices[3 * k..3 * k + 3];
    let normal = |k: usize| {
        let [a, b, c] = [tri(k)[0], tri(k)[1], tri(k)[2]].map(|i| corner(model, i));
        (b - a).cross(c - a).normalize_or_zero()
    };
    let mut worst: Option<(f64, f64, [f32; 3])> = None;
    for shared in edges.values() {
        let [(t1, a, b), (t2, _, _)] = shared.as_slice() else {
            continue;
        };
        let theta = det::acos(f64::from(normal(*t1).dot(normal(*t2)).clamp(-1.0, 1.0)));
        let (e0, e1) = (corner(model, *a), corner(model, *b));
        let length = f64::from((e1 - e0).length());
        if theta < 1e-6 || length <= 0.0 {
            continue;
        }
        // Each triangle's width across the edge: its height on it.
        let width = |k: usize| 2.0 * f64::from(area(model, tri(k))) / length;
        let chord = width(*t1).min(width(*t2));
        // A polygon of chords `chord` turning by θ at each edge stands for a circle of radius
        // r = chord / (2 sin(θ/2)); each chord strays (chord/2)·tan(θ/4) from it.
        let radius = chord / (2.0 * det::sin(theta / 2.0));
        let stray = chord / 2.0 * det::tan(theta / 4.0);
        let limit = (SAGITTA_SHARE * radius).max(SAGITTA_FLOOR);
        if stray > limit && worst.is_none_or(|(s, l, _)| stray - limit > s - l) {
            worst = Some((stray, limit, e0.to_array()));
        }
    }
    worst
}
