//! Item bodies' shapes (G1.6a–c, ADR-0028): a piece as its shape, a bulk as a heap.

use super::*;
use creeer_math::det;
use creeer_physics::world::hull::Hull;
use creeer_sim::items::{Form, Item};

/// The angle a heap of loose matter stands at (rad): 35°, dry sand and gravel's angle of repose.
const REPOSE: f64 = 0.61;

/// A body for `item` with its centre at `at`, at rest: a piece as its shape, a bulk as a heap
/// (a cone at its angle of repose).
pub(super) fn body_of(item: &Item, at: DVec3) -> Body {
    let (shape, friction) = match item {
        Item::Piece(p, _) => (
            match p.form {
                Form::Block { size } => Shape::Box { half: size * 0.5 },
                Form::Ball { radius } => Shape::Sphere { radius },
                Form::Rod { radius, length } => prism(radius, length),
                Form::Lump { size } => lump(size),
            },
            0.6,
        ),
        Item::Bulk(b) => (heap(b.volume()), 0.7),
        Item::Made(a) => (made(a), 0.6),
    };
    let mut body = Body::rigid(shape, item.mass(), at, DT);
    body.structure.orientation[0] = upright(at);
    body.friction = friction;
    body.solver.reset_history(&body.structure);
    body
}

/// A rod as a sixteen-sided prism along x.
fn prism(radius: f64, length: f64) -> Shape {
    let points: Vec<DVec3> = (0..32)
        .map(|k| {
            let a = f64::from(k % 16) * std::f64::consts::TAU / 16.0;
            let (s, c) = det::sin_cos(a);
            let x = if k < 16 { -0.5 } else { 0.5 } * length;
            DVec3::new(x, radius * c, radius * s)
        })
        .collect();
    Shape::Hull(Arc::new(Hull::new(&points).expect("a prism is a hull").0))
}

/// A lump of these axes' lengths (an ellipsoid) as a hull: rings of 12 at five latitudes,
/// and the poles.
fn lump(size: DVec3) -> Shape {
    let half = size * 0.5;
    let mut points = vec![DVec3::new(0.0, 0.0, half.z), DVec3::new(0.0, 0.0, -half.z)];
    for ring in 1..6 {
        let (sl, cl) = det::sin_cos(
            f64::from(ring) * std::f64::consts::PI / 6.0 - std::f64::consts::FRAC_PI_2,
        );
        for k in 0..12 {
            let (s, c) = det::sin_cos(f64::from(k) * std::f64::consts::TAU / 12.0);
            points.push(DVec3::new(half.x * cl * c, half.y * cl * s, half.z * sl));
        }
    }
    Shape::Hull(Arc::new(
        Hull::new(&points).expect("an ellipsoid is a hull").0,
    ))
}

/// A heap of `volume` (m³): a cone at the angle of repose, as a hull.
fn heap(volume: f64) -> Shape {
    // V = π·r²·h/3 with h = r·tan(repose).
    let tan = det::tan(REPOSE);
    let r = det::cbrt(3.0 * volume / (std::f64::consts::PI * tan));
    let mut points: Vec<DVec3> = (0..16)
        .map(|k| {
            let (s, c) = det::sin_cos(f64::from(k) * std::f64::consts::TAU / 16.0);
            DVec3::new(r * c, 0.0, r * s)
        })
        .collect();
    points.push(DVec3::new(0.0, r * tan, 0.0));
    Shape::Hull(Arc::new(Hull::new(&points).expect("a cone is a hull").0))
}

/// A made thing as the hull about its parts' boxes (a tool's haft and head as one body).
fn made(a: &creeer_sim::items::Assembly) -> Shape {
    let mut points = Vec::new();
    for p in &a.parts {
        let half = match p.piece.form {
            Form::Block { size } | Form::Lump { size } => size * 0.5,
            Form::Ball { radius } => DVec3::splat(radius),
            Form::Rod { radius, length } => DVec3::new(length * 0.5, radius, radius),
        };
        for k in 0..8 {
            let corner = DVec3::new(
                if k & 1 == 0 { -half.x } else { half.x },
                if k & 2 == 0 { -half.y } else { half.y },
                if k & 4 == 0 { -half.z } else { half.z },
            );
            points.push(p.at + p.turn * corner);
        }
    }
    Shape::Hull(Arc::new(Hull::new(&points).expect("parts make a hull").0))
}
