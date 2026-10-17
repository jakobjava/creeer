//! The volume of a shape below a plane, exactly, and its centroid (the centre of buoyancy).
//!
//! A sphere's is the spherical cap's. A box's or a convex hull's is the polyhedron the plane
//! cuts off: each face clipped to the water (Sutherland–Hodgman), and the solid summed as
//! tetrahedra from a point on the waterline, where the cut's own face adds nothing.

use creeer_math::glam::DVec3;
use creeer_physics::world::shape::Shape;
use std::f64::consts::PI;

/// The water's plane in a body's frame: points y with `normal`·y < `level` are below it
/// (`normal` unit, pointing up out of the water).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Waterline {
    pub normal: DVec3,
    pub level: f64,
}

impl Waterline {
    /// How deep `y` is below it (m; negative: above).
    pub fn depth(&self, y: DVec3) -> f64 {
        self.level - self.normal.dot(y)
    }
}

/// What of a shape is under water: its volume (m³), centroid (m, the shape's frame), and
/// the area it shows the water moving along `u`: half its wetted faces' area vectors' |·û|
/// summed, the waterline's cut among them (a sphere's, πr² times its share of the volume^⅔).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Wetted {
    pub volume: f64,
    pub centre: DVec3,
    faces: Vec<DVec3>,
    disc: f64,
    /// The area the water's surface cuts from it (m²).
    pub waterplane: f64,
}

impl Wetted {
    /// Its area seen along `direction` (m²).
    pub fn projected(&self, direction: DVec3) -> f64 {
        let d = direction.normalize_or_zero();
        if self.faces.is_empty() {
            return self.disc;
        }
        let cut: DVec3 = -self.faces.iter().sum::<DVec3>();
        0.5 * (self.faces.iter().map(|a| a.dot(d).abs()).sum::<f64>() + cut.dot(d).abs())
    }
}

/// The volume of `shape` (in its own frame) below `water`, and its centroid (m³, m).
pub fn submerged(shape: &Shape, water: &Waterline) -> (f64, DVec3) {
    let w = wetted(shape, water);
    (w.volume, w.centre)
}

/// What of `shape` (in its own frame) is below `water`.
pub fn wetted(shape: &Shape, water: &Waterline) -> Wetted {
    match shape {
        Shape::Sphere { radius } => {
            let (volume, centre) = cap(*radius, water);
            let share = volume / (4.0 / 3.0 * PI * radius * radius * radius);
            let k = (radius + water.depth(DVec3::ZERO)).clamp(0.0, 2.0 * radius);
            let disc = PI
                * radius
                * radius
                * creeer_math::det::cbrt(share)
                * creeer_math::det::cbrt(share);
            Wetted {
                volume,
                centre,
                faces: Vec::new(),
                disc,
                waterplane: PI * k * (2.0 * radius - k),
            }
        }
        Shape::Box { half } => {
            let h = *half;
            let corner = |x: f64, y: f64, z: f64| DVec3::new(x * h.x, y * h.y, z * h.z);
            // Its six faces, each counter-clockwise seen from outside.
            let faces = [
                [
                    corner(1.0, -1.0, -1.0),
                    corner(1.0, 1.0, -1.0),
                    corner(1.0, 1.0, 1.0),
                    corner(1.0, -1.0, 1.0),
                ],
                [
                    corner(-1.0, -1.0, -1.0),
                    corner(-1.0, -1.0, 1.0),
                    corner(-1.0, 1.0, 1.0),
                    corner(-1.0, 1.0, -1.0),
                ],
                [
                    corner(-1.0, 1.0, -1.0),
                    corner(-1.0, 1.0, 1.0),
                    corner(1.0, 1.0, 1.0),
                    corner(1.0, 1.0, -1.0),
                ],
                [
                    corner(-1.0, -1.0, -1.0),
                    corner(1.0, -1.0, -1.0),
                    corner(1.0, -1.0, 1.0),
                    corner(-1.0, -1.0, 1.0),
                ],
                [
                    corner(-1.0, -1.0, 1.0),
                    corner(1.0, -1.0, 1.0),
                    corner(1.0, 1.0, 1.0),
                    corner(-1.0, 1.0, 1.0),
                ],
                [
                    corner(-1.0, -1.0, -1.0),
                    corner(-1.0, 1.0, -1.0),
                    corner(1.0, 1.0, -1.0),
                    corner(1.0, -1.0, -1.0),
                ],
            ];
            polyhedron(faces.iter().map(|f| f.iter().copied()), water)
        }
        Shape::Hull(hull) => polyhedron((0..hull.faces().len()).map(|f| hull.ring(f)), water),
    }
}

/// A sphere's cap below the plane: height k = r + depth of the centre (clamped to the
/// sphere), V = π·k²·(3r − k)/3, its centroid 3·(2r − k)²/(4·(3r − k)) below the centre.
fn cap(r: f64, water: &Waterline) -> (f64, DVec3) {
    let k = (r + water.depth(DVec3::ZERO)).clamp(0.0, 2.0 * r);
    if k <= 0.0 {
        return (0.0, DVec3::ZERO);
    }
    let volume = PI * k * k * (3.0 * r - k) / 3.0;
    let below = 3.0 * (2.0 * r - k) * (2.0 * r - k) / (4.0 * (3.0 * r - k));
    (volume, -water.normal * below)
}

/// A convex polyhedron's part below the plane, from its faces' corners.
fn polyhedron<F, C>(faces: F, water: &Waterline) -> Wetted
where
    F: Iterator<Item = C>,
    C: Iterator<Item = DVec3>,
{
    let origin = water.normal * water.level;
    let (mut volume, mut moment) = (0.0, DVec3::ZERO);
    let mut areas = Vec::with_capacity(8);
    let mut clipped: Vec<DVec3> = Vec::with_capacity(8);
    let mut ring: Vec<DVec3> = Vec::with_capacity(8);
    for face in faces {
        ring.clear();
        ring.extend(face);
        clipped.clear();
        for (k, &a) in ring.iter().enumerate() {
            let b = ring[(k + 1) % ring.len()];
            let (da, db) = (water.depth(a), water.depth(b));
            if da >= 0.0 {
                clipped.push(a);
            }
            if (da >= 0.0) != (db >= 0.0) {
                clipped.push(a + (b - a) * (da / (da - db)));
            }
        }
        if clipped.len() < 3 {
            continue;
        }
        let p0 = clipped[0] - origin;
        let mut area = DVec3::ZERO;
        for w in clipped[1..].windows(2) {
            let (p1, p2) = (w[0] - origin, w[1] - origin);
            area += (w[0] - clipped[0]).cross(w[1] - clipped[0]) * 0.5;
            let v = p0.dot(p1.cross(p2)) / 6.0;
            volume += v;
            moment += (p0 + p1 + p2) * (v / 4.0);
        }
        areas.push(area);
    }
    if volume <= 0.0 {
        return Wetted::default();
    }
    let waterplane = areas.iter().sum::<DVec3>().length();
    Wetted {
        volume,
        centre: origin + moment / volume,
        faces: areas,
        disc: 0.0,
        waterplane,
    }
}
