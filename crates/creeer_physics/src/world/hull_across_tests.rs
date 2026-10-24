//! An edge lying across a face (`hull_sat::edge_on_face`, G2.0): a felled log lying across
//! another touches it along a line, held by the face's patch whichever end is the deeper.

use super::{deepest, summary};
use crate::world::contact::law::FACING;
use crate::world::hull::Hull;
use crate::world::hull_sat::hull_hull;
use crate::world::narrow::pose;
use crate::world::rotation;
use creeer_math::glam::{DQuat, DVec3};

use creeer_math::det;

/// A twelve-sided log of radius 0.1 m and length 3 m along x, centred on the origin, turned
/// by `phase` (rad) about its axis (0: a face on top and below).
fn log(phase: f64) -> (Hull, DVec3) {
    let mut points = Vec::new();
    for k in 0..12 {
        let (s, c) = det::sin_cos(phase + (15.0 + 30.0 * f64::from(k)).to_radians());
        for x in [-1.5, 1.5] {
            points.push(DVec3::new(x, 0.1 * s, 0.1 * c));
        }
    }
    Hull::new(&points).expect("a log")
}

/// Log B lies across log A at 33°, an edge of B down on A's top face, `tilt` (rad) about the
/// horizontal across B so that one end of the line is pressed deeper; `depth` (m) where B
/// crosses A's axis.
fn across(tilt: f64, depth: f64) -> Vec<crate::world::shape::RawContact> {
    let (a, ca) = log(0.0);
    let (b, cb) = log(15f64.to_radians());
    let top = 0.1 * det::cos(15f64.to_radians());
    let lowest = 0.1;
    let along = rotation(DVec3::Y, 33f64.to_radians());
    let turn = along * rotation(DVec3::Z, tilt);
    let at_a = pose(ca, DQuat::IDENTITY);
    let at_b = pose(DVec3::new(0.0, top + lowest - depth, 0.0) + turn * cb, turn);
    hull_hull(at_a, &a, at_b, &b, 0.005)
}

/// B's edge presses into A's face as one patch over the whole crossing (A's face is 5.2 cm
/// wide, B's faces beside its edge as wide, crossing at 33°: about 50 cm² seen from above),
/// with A's face's normal; tilted either way, the same patch, the deepest point of the line
/// at either end. (Before G2.0: one point, at the deeper end, which hopped from end to end
/// as the log rocked.)
#[test]
fn an_edge_lying_across_a_face_presses_along_the_whole_line() {
    for tilt in [-2e-3, -1e-3, 0.0, 1e-3, 2e-3] {
        let contacts = across(tilt, 50e-6);
        assert_eq!(contacts.len(), 4, "tilt {tilt}: one patch, {contacts:?}");
        assert!(contacts.iter().all(|c| c.patch.is_some()), "tilt {tilt}");
        let normal = contacts[0].normal;
        assert!(normal.y.abs() > 1.0 - FACING, "tilt {tilt}: {normal}");
        let s = summary(&contacts);
        let area = s[0].1;
        assert!(area > 30e-4 && area < 60e-4, "tilt {tilt}: {area} m²");
        // The line's deeper end: 50 µm at the axis, plus the tilt over half the crossing.
        let deep = deepest(&contacts);
        let reach = 0.5 * 0.052 / det::sin(33f64.to_radians());
        let want = -(50e-6 + tilt.abs() * reach);
        assert!(
            (deep - want).abs() < 2e-6,
            "tilt {tilt}: {deep} m, ~{want} m"
        );
    }
}
