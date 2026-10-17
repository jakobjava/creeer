//! A hull's contacts with the ground (`ground.rs`), as a box's: every face turned towards the
//! ground within the margin is a patch over its rectangle (`hull_narrow.rs`), pressing with
//! the ground's depth measured across it (`ground_rise.rs`). Points at most
//! [`BOX_SAMPLE_SPACING`] apart inside the faces and along the edges count too, where the
//! ground rises into the hull beyond what its patches see: a bump, a point contact of that
//! size. (Inside the faces and along the edges apart: a point on an
//! edge sampled with both its faces would press twice.)

use super::ground::{BOX_SAMPLE_SPACING, BULGE, Ground, estimate, point_contact, project};
use super::ground_rise::{Pressing, face_gap};
use super::ground_sample::{Feet, measure_face};
use super::hull::Hull;
use super::hull_narrow::{Placed, domain};
use super::shape::{Pose, RawContact};
use super::surface::Feature;
use creeer_math::glam::DVec3;

/// Contacts of a hull at `pose` with the ground (gap below `margin`).
pub fn hull_contacts(ground: &dyn Ground, pose: Pose, hull: &Hull, margin: f64) -> Vec<RawContact> {
    let placed = Placed::new(hull, pose);
    let gaps: Vec<f64> = placed
        .vertices
        .iter()
        .map(|&v| estimate(ground, v).unwrap_or(f64::INFINITY))
        .collect();
    let (lowest, &deepest) = gaps
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.total_cmp(b.1))
        .expect("a hull has corners");
    // Only if the hull is close enough for the ground to reach it.
    if deepest > margin + hull.bounding_radius()
        || project(ground, placed.vertices[lowest]).is_none()
    {
        return Vec::new();
    }
    // The faces that press as patches (`faces`), and other surface points where the ground
    // rises beyond what the patches see.
    let (mut found, patched) = faces(ground, &placed, margin);
    let spacing = |length: f64| {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // small, positive
        let n = ((length / BOX_SAMPLE_SPACING).ceil() as u32).clamp(2, 16);
        n
    };
    let mut bump = |p: DVec3, plane: f64, feature: Feature| {
        // On a face pressing as a patch, the gap its patch sees there: a bump is what rises
        // beyond.
        let plane = face_gap(&patched, p).unwrap_or(plane);
        let Some(gap) = estimate(ground, p) else {
            return;
        };
        if gap < margin && gap < plane - BULGE {
            found.extend(point_contact(ground, p, 0.0, margin).map(|mut c| {
                c.features = [feature, Feature::Round(BOX_SAMPLE_SPACING)];
                c.inset = (plane - BULGE).min(0.0);
                c
            }));
        }
    };
    for f in 0..hull.faces().len() {
        let ring = placed.ring(f);
        let normal = placed.normals[f];
        let rect = placed.rectangle(f);
        let corner_gaps = rect.map(|c| estimate(ground, c).unwrap_or(f64::INFINITY));
        let (ns, nt) = (
            spacing((rect[1] - rect[0]).length()),
            spacing((rect[3] - rect[0]).length()),
        );
        for i in 1..ns {
            for j in 1..nt {
                let (s, t) = (f64::from(i) / f64::from(ns), f64::from(j) / f64::from(nt));
                let w = [(1.0 - s) * (1.0 - t), s * (1.0 - t), s * t, (1.0 - s) * t];
                let p: DVec3 = (0..4).map(|k| rect[k] * w[k]).sum();
                if !strictly_inside(&ring, normal, p) {
                    continue;
                }
                // The corners' gaps interpolated to p: the gap a flat ground would have.
                let plane: f64 = (0..4).map(|k| corner_gaps[k] * w[k]).sum();
                bump(p, plane, Feature::Face);
            }
        }
    }
    for edge in hull.edges() {
        let [u, v] = edge.ends;
        let (p, q) = (placed.vertices[u], placed.vertices[v]);
        let n = spacing((q - p).length());
        for k in 1..n {
            let t = f64::from(k) / f64::from(n);
            bump(
                p + (q - p) * t,
                gaps[u] + (gaps[v] - gaps[u]) * t,
                Feature::Edge,
            );
        }
    }
    found
}

/// Whether `p` (in the plane of the convex polygon `ring`, normal `normal`, its corners
/// counter-clockwise seen from outside) lies inside it, off its edges.
fn strictly_inside(ring: &[DVec3], normal: DVec3, p: DVec3) -> bool {
    let len = ring.len();
    (0..len).all(|k| {
        let (a, b) = (ring[k], ring[(k + 1) % len]);
        (b - a).cross(p - a).dot(normal) > 1e-9 * (b - a).length_squared()
    })
}

/// The faces of a hull turned towards the ground within `margin` of it: a patch each over its
/// rectangle, measured over the ground as a box's (`ground_sample.rs`).
fn faces(
    ground: &dyn Ground,
    placed: &Placed<'_>,
    margin: f64,
) -> (Vec<RawContact>, Vec<Pressing>) {
    let mut feet = Feet::new(ground);
    let mut out = Vec::new();
    let mut patched = Vec::new();
    for f in 0..placed.hull.faces().len() {
        let rect = placed.rectangle(f);
        let ring = placed.ring(f);
        let points = domain(&rect, &ring);
        let Some(face) = measure_face(&mut feet, &rect, points.points(), placed.normals[f], margin)
        else {
            continue;
        };
        out.extend(face.under.corners(&rect, &face.domain, &mut feet));
        patched.push(Pressing {
            corners: rect,
            under: face.under,
            domain: face.domain,
        });
    }
    (out, patched)
}
