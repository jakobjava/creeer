//! The sandbox's unit primitives, smooth (G1.3c, ADR-0026 decision 11, `docs/art-style.md`
//! pillar 9, §7): a box with rounded edges, a cylinder with rounded rims and a sphere, each in
//! levels of detail chosen by their error on screen (`renderer/levels.rs`).
//!
//! Each fits the cube of side 1 around the origin and is scaled per instance. Rounds are built
//! as they are drawn: a bevelled vertex at `p` with normal `n` is placed at `p·s + (n − 2p)·r`,
//! its corner of the inner shape plus its normal times the bevel radius `r`. So the box's
//! edges and corners and the cylinder's rims are true rounds of `r` in metres however a part
//! is scaled, and the sphere (`n = 2p`) is unmoved. A round of `k` segments a quarter turn
//! strays `r(1 − cos(π/4k))` from its arc: at least 1 % of the radius or 1 mm, as authored
//! models must (`creeer_art::model_check`), wherever that is half a pixel or more.

use crate::mesh::{MeshData, MeshVertex};
use creeer_math::glam::Vec3;
use std::f32::consts::{PI, TAU};

/// A bevel's radius as a share of a part's smallest side…
pub const BEVEL_SHARE: f32 = 0.08;
/// …and at most (m).
pub const BEVEL_MAX: f32 = 0.025;

/// The segments around of each level of the cylinder and the sphere, coarsest first.
pub const LEVELS: [u32; 10] = [6, 8, 12, 16, 24, 32, 48, 64, 96, 128];
/// The segments a quarter round of each rounded level of the box, coarsest first (after a
/// sharp box).
pub const BOX_LEVELS: [u32; 6] = [1, 2, 3, 4, 6, 8];
/// A cylinder's rims, the rounds of its bevel radius, are at most this share of its radius:
/// 8 % of its smallest side over half its diameter.
const RIM_SHARE: f32 = 2.0 * BEVEL_SHARE;

/// How a primitive's silhouette error is measured (`renderer/levels.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Round {
    /// The box: its error is a share of its bevel radius.
    Box,
    /// Round about its Y axis: a share of its radius, half its larger X or Z scale.
    Cylinder,
    /// Round every way: a share of its radius, half its largest scale.
    Sphere,
}

/// The bevel radius of a part scaled by `scale` (m).
pub fn bevel_radius(scale: Vec3) -> f32 {
    (BEVEL_SHARE * scale.min_element()).min(BEVEL_MAX)
}

/// Where `v` is drawn in its instance's space, for the instance's `scale` (the shaders'
/// `shaped`, `globals.wgsl`).
pub fn shaped(v: &MeshVertex, scale: Vec3) -> Vec3 {
    let p = Vec3::from(v.position);
    let r = if v.is_bevelled() {
        bevel_radius(scale)
    } else {
        0.0
    };
    p * scale + (Vec3::from(v.normal) - 2.0 * p) * r
}

#[allow(clippy::cast_precision_loss)] // at most a few hundred segments
fn turn(i: u32, n: u32) -> f32 {
    TAU * i as f32 / n as f32
}

/// The silhouette error of a circle of radius 1 drawn with `segments` around: its sagitta.
pub fn sagitta(segments: u32) -> f32 {
    1.0 - (turn(1, segments) / 2.0).cos()
}

/// The error of a round of radius 1 drawn with `segments` a quarter turn.
pub fn round_error(segments: u32) -> f32 {
    sagitta(4 * segments)
}

/// The error of a sphere of radius 1 drawn with `segments` around and half as many rings:
/// its faces' centres lie deepest, about twice the sagitta in.
pub fn sphere_error(segments: u32) -> f32 {
    1.0 - (PI / (2 * (segments / 2)) as f32).cos() * (turn(1, segments) / 2.0).cos()
}

/// The segments a quarter round the rims of a cylinder of `segments` around need, so they
/// stray no further than its sides: none (sharp rims) while a sharp rim's √2 − 1 of the bevel
/// radius is within it.
pub fn rim_segments(segments: u32) -> u32 {
    let allowed = sagitta(segments) / RIM_SHARE;
    if std::f32::consts::SQRT_2 - 1.0 <= allowed {
        return 0;
    }
    (1..)
        .find(|&k| round_error(k) <= allowed)
        .expect("a fine enough round")
}

/// The unit box's levels of detail, coarsest first, each with its silhouette error as a share
/// of its bevel radius: a sharp box (√3 − 1 at its corners), then rounder ones.
pub fn box_levels() -> Vec<(MeshData, f32)> {
    let sharp = (MeshData::cuboid(Vec3::splat(0.5)), 3_f32.sqrt() - 1.0);
    let rounded = BOX_LEVELS
        .iter()
        .map(|&k| (MeshData::rounded_box(k), rounded_box_error(k)));
    std::iter::once(sharp).chain(rounded).collect()
}

/// The unit cylinder's levels of detail, coarsest first, each with its silhouette error as a
/// share of its radius.
pub fn cylinder_levels() -> Vec<(MeshData, f32)> {
    LEVELS
        .iter()
        .map(|&n| (MeshData::rounded_cylinder(n, rim_segments(n)), sagitta(n)))
        .collect()
}

/// The unit sphere's levels of detail, coarsest first, with their errors.
pub fn sphere_levels() -> Vec<(MeshData, f32)> {
    LEVELS
        .iter()
        .map(|&n| (MeshData::uv_sphere(0.5, n, n / 2), sphere_error(n)))
        .collect()
}

/// The normals of one corner of a box rounded `k` segments a quarter turn, at (+, +, +): for
/// each `(i, j)` with `i + j ≤ k`, at weights `sin(πm/2k)` of the three axes (`m` = i, j and
/// k − i − j), so its edges step evenly, as the edge strips do. Row by row in `i`.
fn corner_normals(k: u32) -> Vec<Vec3> {
    let w = |m: u32| (turn(m, 4 * k)).sin();
    let mut out = Vec::new();
    for i in 0..=k {
        for j in 0..=k - i {
            out.push(Vec3::new(w(i), w(j), w(k - i - j)).normalize());
        }
    }
    out
}

/// The index of `(i, j)` in `corner_normals(k)`.
fn corner_index(k: u32, i: u32, j: u32) -> u32 {
    // Rows 0..i hold (k + 1) + k + … + (k + 2 − i) normals.
    i * (2 * k + 3 - i) / 2 + j
}

/// The corner patch's triangles, as `corner_normals` indices.
fn corner_triangles(k: u32) -> Vec<[u32; 3]> {
    let at = |i, j| corner_index(k, i, j);
    let mut out = Vec::new();
    for i in 0..k {
        for j in 0..k - i {
            out.push([at(i, j), at(i + 1, j), at(i, j + 1)]);
            if i + j + 1 < k {
                out.push([at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)]);
            }
        }
    }
    out
}

/// The silhouette error of a box rounded `k` segments a quarter turn, as a share of its
/// bevel radius: its corners' deepest facets, or its edges' sagitta.
pub fn rounded_box_error(k: u32) -> f32 {
    let normals = corner_normals(k);
    corner_triangles(k)
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| normals[i as usize]);
            1.0 - (b - a).cross(c - a).normalize().dot(a)
        })
        .fold(round_error(k), f32::max)
}

#[path = "primitives_shapes.rs"]
mod shapes;

#[cfg(test)]
#[path = "primitives_tests.rs"]
mod tests;
