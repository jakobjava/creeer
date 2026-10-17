//! The cube-sphere: six faces of a cube projected onto the sphere.
//!
//! A direction is found on the face its largest component points to. On that face its
//! gnomonic coordinates are `g = (d·u)/(d·n)` and `(d·v)/(d·n)`, each in [−1, 1]. A grid that is
//! uniform in `g` gives cells twice as large (in angle) at the face centre as at its edges, so
//! the face coordinates are warped: `s = g·(1.2 − 0.2·g²)`. Its slope falls from 1.2 to 0.6
//! like the equal-angle mapping `(4/π)·atan(g)`, so grid cells along each face axis
//! stay within ±9 % of one size, but it needs only a division and a few multiplications
//! per lookup and no library function (bit-identical everywhere). The inverse, needed only to
//! place grid samples, is a fixed number of Newton steps.
//!
//! Face coordinates outside [−1, 1] are valid: they continue onto the neighbouring faces,
//! which lets grids on one face carry a border of samples from the next one.

use creeer_math::glam::DVec3;

/// Point on the cube-sphere: face and warped face coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubeCoord {
    /// Face 0..6: +X, −X, +Y, −Y, +Z, −Z.
    pub face: u8,
    /// First face coordinate, [−1, 1] on the face.
    pub s: f64,
    /// Second face coordinate, [−1, 1] on the face.
    pub t: f64,
}

/// Normal and the two in-face axes of each face, with `u × v = n`.
const FACES: [(DVec3, DVec3, DVec3); 6] = [
    (DVec3::X, DVec3::NEG_Z, DVec3::Y),
    (DVec3::NEG_X, DVec3::Z, DVec3::Y),
    (DVec3::Y, DVec3::X, DVec3::NEG_Z),
    (DVec3::NEG_Y, DVec3::X, DVec3::Z),
    (DVec3::Z, DVec3::X, DVec3::Y),
    (DVec3::NEG_Z, DVec3::NEG_X, DVec3::Y),
];

/// Warped coordinate from a gnomonic one.
pub fn warp(g: f64) -> f64 {
    g * (1.2 - 0.2 * g * g)
}

/// Gnomonic coordinate from a warped one (inverse of [`warp`] for |s| ≤ 1.5).
pub fn unwarp(s: f64) -> f64 {
    // The warp is monotonic for |g| < √2 (|s| < 1.13) and turns over beyond, so border
    // samples far outside a face are clamped; nothing uses |s| > 1.1.
    let s = s.clamp(-1.1, 1.1);
    let mut g = s;
    for _ in 0..8 {
        let f = warp(g) - s;
        let slope = 1.2 - 0.6 * g * g;
        g -= f / slope;
    }
    g
}

impl CubeCoord {
    /// The face a direction (not necessarily unit) lies on and its coordinates there.
    pub fn from_direction(d: DVec3) -> Self {
        let a = d.abs();
        // Ties go to the earlier axis so every direction has exactly one face.
        let face = if a.x >= a.y && a.x >= a.z {
            if d.x >= 0.0 { 0 } else { 1 }
        } else if a.y >= a.z {
            if d.y >= 0.0 { 2 } else { 3 }
        } else if d.z >= 0.0 {
            4
        } else {
            5
        };
        Self::on_face(face, d)
    }

    /// Coordinates of direction `d` projected onto `face` (may lie outside [−1, 1]).
    pub fn on_face(face: u8, d: DVec3) -> Self {
        let (n, u, v) = FACES[usize::from(face)];
        let depth = d.dot(n);
        Self {
            face,
            s: warp(d.dot(u) / depth),
            t: warp(d.dot(v) / depth),
        }
    }

    /// Gradients of this point's face coordinates (`s`, `t` on its face) with respect to the
    /// point `p` it was found from (any length: the coordinates depend on direction only).
    pub fn gradients(&self, p: DVec3) -> (DVec3, DVec3) {
        let (n, u, v) = FACES[usize::from(self.face)];
        let depth = p.dot(n);
        // g = (p·a)/(p·n): ∇g = (a − g·n)/(p·n); s = warp(g): ds/dg = 1.2 − 0.6·g².
        let along = |a: DVec3| {
            let g = p.dot(a) / depth;
            (a - n * g) * ((1.2 - 0.6 * g * g) / depth)
        };
        (along(u), along(v))
    }

    /// Unit direction of this point.
    pub fn direction(&self) -> DVec3 {
        let (n, u, v) = FACES[usize::from(self.face)];
        (n + u * unwarp(self.s) + v * unwarp(self.t)).normalize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directions() -> impl Iterator<Item = DVec3> {
        // A deterministic spread of directions, including face edges and corners.
        let mut out = vec![
            DVec3::new(1.0, 1.0, 0.0),
            DVec3::new(1.0, 1.0, 1.0),
            DVec3::new(-1.0, 1.0, -1.0),
            DVec3::new(0.0, -1.0, 1.0),
        ];
        for i in 0..2000u32 {
            let t = f64::from(i);
            out.push(DVec3::new(
                creeer_math::det::sin(t * 1.37),
                creeer_math::det::cos(t * 0.91),
                creeer_math::det::sin(t * 2.13 + 0.5),
            ));
        }
        out.into_iter().filter(|d| d.length() > 1e-6)
    }

    #[test]
    fn faces_have_right_handed_axes() {
        for (n, u, v) in FACES {
            assert_eq!(u.cross(v), n);
        }
    }

    #[test]
    fn warp_round_trips_and_keeps_face_edges() {
        assert!((warp(1.0) - 1.0).abs() < 1e-15);
        assert!((warp(-1.0) + 1.0).abs() < 1e-15);
        for i in -110..=110 {
            let s = f64::from(i) / 100.0;
            assert!((warp(unwarp(s)) - s).abs() < 1e-14, "s = {s}");
        }
    }

    #[test]
    fn direction_round_trips() {
        for d in directions() {
            let c = CubeCoord::from_direction(d);
            assert!(c.s.abs() <= 1.0 + 1e-12 && c.t.abs() <= 1.0 + 1e-12);
            let back = c.direction();
            assert!(
                (back - d.normalize()).length() < 1e-12,
                "{d} -> {c:?} -> {back}"
            );
        }
    }

    #[test]
    fn neighbouring_face_coordinates_describe_the_same_direction() {
        // A point just past the +X face's edge, read on +X, is the same direction as on +Y.
        let d = DVec3::new(1.0, 1.05, 0.3);
        let on_x = CubeCoord::on_face(0, d);
        assert!(on_x.s.abs() < 1.0 && on_x.t > 1.0);
        let on_y = CubeCoord::from_direction(d);
        assert_eq!(on_y.face, 2);
        assert!((on_x.direction() - on_y.direction()).length() < 1e-12);
    }

    #[test]
    fn grid_cells_have_nearly_uniform_size_along_axes() {
        // Angle covered by a step of the face coordinate along the face's centre line.
        let step = 1e-4;
        let mut sizes = Vec::new();
        for i in -100..100 {
            let s = f64::from(i) / 100.0;
            let a = CubeCoord { face: 4, s, t: 0.0 }.direction();
            let b = CubeCoord {
                face: 4,
                s: s + step,
                t: 0.0,
            }
            .direction();
            sizes.push(a.angle_between(b) / step);
        }
        let (min, max) = sizes
            .iter()
            .fold((f64::MAX, f64::MIN), |(lo, hi), &x| (lo.min(x), hi.max(x)));
        // ±9 % around the mean (the equal-angle map is exactly uniform here; gnomonic is 2:1).
        assert!(max / min < 1.2, "cell sizes {min}..{max}");
    }

    #[test]
    fn face_coordinate_gradients_match_finite_differences() {
        for d in directions() {
            let p = d * 1234.5;
            let c = CubeCoord::from_direction(p);
            let (gs, gt) = c.gradients(p);
            let h = 1e-4;
            for axis in [DVec3::X, DVec3::Y, DVec3::Z] {
                let (a, b) = (
                    CubeCoord::on_face(c.face, p + axis * h),
                    CubeCoord::on_face(c.face, p - axis * h),
                );
                let (ds, dt) = ((a.s - b.s) / (2.0 * h), (a.t - b.t) / (2.0 * h));
                assert!(
                    (ds - gs.dot(axis)).abs() < 1e-9 && (dt - gt.dot(axis)).abs() < 1e-9,
                    "{d}: {ds} {dt} vs {gs} {gt}"
                );
            }
        }
    }
}
