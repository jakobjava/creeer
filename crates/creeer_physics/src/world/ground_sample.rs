//! Measuring the ground under a face (`ground_rise.rs`): its points projected to the ground
//! once each, the ground's curvature under the face from their normals, and the face's
//! triangles over it.
//!
//! The terrain is costly to sample (a few microseconds a point), so every point is measured
//! once per body and step: the face's centroid (where it takes the ground under it), its
//! outline (from whose normals the curvature across it follows, as the triangles need it),
//! its triangles' corners, and its patch's corners, which are points of the outline for a
//! box's face and shared with the box's other faces. (Projecting every face's centroid,
//! taking the curvature from normals 2 cm apart and each patch's corners apart, 50 crates
//! landing on the terrain spent 2 ms a step finding their contacts, F5.)

use super::contact::law::FACING;
use super::ground::{BULGE, FLAT, Ground, project};
use super::ground_rise::{FaceGround, reach};
use super::shape::{Domain, Sampled};
use creeer_math::glam::{DMat3, DVec3};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

/// How far the depth interpolated across a triangle may be off the ground's (m): the
/// [`BULGE`] beyond which the ground counts as a bump of its own.
const RISE_TOLERANCE: f64 = BULGE;

/// Triangles of a face's domain at most (a 0.5 m square face on ground curving by 1 m⁻¹
/// takes 64; flat ground none).
const MOST_TRIANGLES: usize = 1024;

/// The ground under points of one body, each projected once (`ground::project`).
pub(super) struct Feet<'g> {
    ground: &'g dyn Ground,
    found: HashMap<[u64; 3], Option<(DVec3, DVec3)>>,
}

impl<'g> Feet<'g> {
    /// None measured yet.
    pub fn new(ground: &'g dyn Ground) -> Self {
        Self {
            ground,
            found: HashMap::new(),
        }
    }

    /// The ground's surface point nearest `p` and its outward normal there.
    pub fn at(&mut self, p: DVec3) -> Option<(DVec3, DVec3)> {
        let ground = self.ground;
        *self
            .found
            .entry(p.to_array().map(f64::to_bits))
            .or_insert_with(|| project(ground, p))
    }
}

/// A face measured over the ground: the ground under it and its domain.
pub(super) struct Measured {
    /// The ground under the face.
    pub under: FaceGround,
    /// Its domain, with the ground under its triangles' corners.
    pub domain: Domain,
}

/// The face with corners `corners` (a parallelogram, in order), domain polygon `points` (s,
/// t, in order) and outward normal `normal`, measured over the ground: none if it is not
/// turned towards the ground under its centroid, or no part of it is within `margin` of it
/// (its points' least gap, less the most the ground can rise between them, then the least
/// its triangles' corners measure: a hump under its middle presses it).
pub(super) fn measure_face(
    feet: &mut Feet<'_>,
    corners: &[DVec3; 4],
    points: &[[f64; 2]],
    normal: DVec3,
    margin: f64,
) -> Option<Measured> {
    let edges = (corners[1] - corners[0], corners[3] - corners[0]);
    let at = |[s, t]: [f64; 2]| corners[0] + edges.0 * s + edges.1 * t;
    let centre = centre(points);
    let (q, up) = feet.at(at(centre))?;
    // Turned towards the ground by the normal its patch weighs its facing with
    // (`Patch::new`): it enters as that weight rises from zero.
    if normal.dot(up) >= -FACING {
        return None;
    }
    let outline: Vec<(DVec3, (DVec3, DVec3))> = points
        .iter()
        .filter_map(|&p| feet.at(at(p)).map(|f| (at(p), f)))
        .collect();
    let under = FaceGround {
        at: q,
        up,
        bend: Some(fit(q, up, &outline)),
    };
    let lowest = outline
        .iter()
        .map(|&(p, (foot, n))| n.dot(p - foot))
        .fold(up.dot(at(centre) - q), f64::min);
    if lowest - reach(corners, &under) >= margin {
        return None;
    }
    let (domain, least) = sampled(corners, points, &under, feet);
    (least < margin).then_some(Measured { under, domain })
}

/// The centre of the polygon `points` (the mean of its corners).
fn centre(points: &[[f64; 2]]) -> [f64; 2] {
    #[allow(clippy::cast_precision_loss)] // a few corners
    let n = points.len() as f64;
    points
        .iter()
        .fold([0.0; 2], |c, p| [c[0] + p[0], c[1] + p[1]])
        .map(|c| c / n)
}

/// The ground's shape operator under a face, from the normals under its outline: near the
/// surface point `q` with normal `up` the normal is up − S·t at a tangential offset t, fitted
/// to them by least squares (a symmetric S across `up`). Flat below [`FLAT`]: on a plane the
/// normals differ by rounding.
fn fit(q: DVec3, up: DVec3, outline: &[(DVec3, (DVec3, DVec3))]) -> DMat3 {
    let (t1, t2) = up.any_orthonormal_pair();
    // S across up as [[a, b], [b, c]]: −(a·x + b·y) = n·t1, −(b·x + c·y) = n·t2.
    let (mut ata, mut atb) = (DMat3::ZERO, DVec3::ZERO);
    for &(_, (foot, n)) in outline {
        let d = foot - q;
        let (x, y) = (t1.dot(d), t2.dot(d));
        let (gx, gy) = (t1.dot(n - up), t2.dot(n - up));
        for (row, g) in [(DVec3::new(x, y, 0.0), gx), (DVec3::new(0.0, x, y), gy)] {
            ata += DMat3::from_cols(row * row.x, row * row.y, row * row.z);
            atb -= row * g;
        }
    }
    if ata.determinant().abs() < 1e-24 {
        return DMat3::ZERO;
    }
    let [a, b, c] = (ata.inverse() * atb).to_array();
    if a.abs().max(b.abs()).max(c.abs()) < FLAT {
        return DMat3::ZERO;
    }
    let outer = |u: DVec3, v: DVec3| DMat3::from_cols(u * v.x, u * v.y, u * v.z);
    outer(t1, t1) * a + (outer(t1, t2) + outer(t2, t1)) * b + outer(t2, t2) * c
}

/// A face's domain over curved ground, and the least gap its patch sees across it: the
/// polygon `points` (s, t in the parallelogram `corners`, in order), fanned from its centre
/// into triangles split level by level until the depth interpolated across each is within
/// [`RISE_TOLERANCE`] of the ground's (along an edge e it is off by |eᵀSe|/8 at most, S the
/// curvature of the ground `under` the face), each of their corners measured to the ground
/// under it (`Domain::Sampled`). The patch presses with the ground's own depth there, a
/// crest under the middle of a face as much as its corners: it rests on a hump as a rocking
/// chair does, its pressed part moving as it rocks. Each corner's depth moves within the
/// step as the ground under it does, to second order, so the step ends where the next one
/// finds it. On flat ground the face as it is, against the plane under it. (Points under the
/// face as bumps of their own were fixed points of the face for the step: a rock balanced
/// on one and tipped off, again and again. A quadric for the whole face was off by
/// millimetres 20 cm from where it was taken, on 10 cm bumps 2 m apart: a rock rested on a
/// bump past it as on a pivot and turned on it for seconds, and taken where the face
/// pressed, the quadric of a slab over a small bump on flat ground put the ground 15 cm into
/// it. Rises sampled at detection and moved with such a quadric within the step did not end
/// where the ground was: energy appeared from step to step, F5.)
pub(super) fn sampled(
    corners: &[DVec3; 4],
    points: &[[f64; 2]],
    under: &FaceGround,
    feet: &mut Feet<'_>,
) -> (Domain, f64) {
    let edges = (corners[1] - corners[0], corners[3] - corners[0]);
    let at = |[s, t]: [f64; 2]| corners[0] + edges.0 * s + edges.1 * t;
    let bend = match under.bend {
        Some(bend) if bend != DMat3::ZERO => bend,
        _ => {
            let least = points
                .iter()
                .map(|&p| under.gap(at(p)))
                .fold(f64::INFINITY, f64::min);
            return (Domain::part(points), least);
        }
    };
    let sag = |e: DVec3| e.dot(bend * e).abs() / 8.0;
    let mid = |p: [f64; 2], q: [f64; 2]| [0.5 * (p[0] + q[0]), 0.5 * (p[1] + q[1])];
    let centre = centre(points);
    // Split level by level (first in, first out): a face too large for [`MOST_TRIANGLES`] at
    // the tolerance stops at a level all over, not fine in one corner and coarse elsewhere.
    let mut pending: VecDeque<[[f64; 2]; 3]> = (0..points.len())
        .map(|k| [centre, points[k], points[(k + 1) % points.len()]])
        .collect();
    let mut vertices: Vec<[f64; 2]> = Vec::new();
    let mut index = HashMap::new();
    let mut triangles = Vec::new();
    while let Some(t) = pending.pop_front() {
        let off = (0..3)
            .map(|j| sag(at(t[j]) - at(t[(j + 1) % 3])))
            .fold(0.0, f64::max);
        if off > RISE_TOLERANCE && triangles.len() + pending.len() + 4 <= MOST_TRIANGLES {
            let (a, b, c) = (mid(t[0], t[1]), mid(t[1], t[2]), mid(t[2], t[0]));
            pending.extend([[t[0], a, c], [a, t[1], b], [c, b, t[2]], [a, b, c]]);
        } else {
            // Shared corners once: the midpoints of an edge two triangles share are the same.
            triangles.push(t.map(|p| {
                *index.entry(p.map(f64::to_bits)).or_insert_with(|| {
                    vertices.push(p);
                    vertices.len() - 1
                })
            }));
        }
    }
    let feet: Vec<(DVec3, DVec3)> = vertices
        .iter()
        .map(|&v| {
            let p = at(v);
            feet.at(p)
                .unwrap_or_else(|| (p - under.normal(p) * under.gap(p), under.normal(p)))
        })
        .collect();
    let least = vertices
        .iter()
        .zip(&feet)
        .map(|(&v, &(foot, n))| n.dot(at(v) - foot))
        .fold(f64::INFINITY, f64::min);
    let domain = Domain::Sampled(Arc::new(Sampled {
        points: points.to_vec(),
        vertices,
        feet,
        bend,
        triangles,
    }));
    (domain, least)
}
